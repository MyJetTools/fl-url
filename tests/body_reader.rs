//! `get_body` takes the body out of a response as its reader: `FlUrlBodyReader` reads it
//! piece by piece with `next_item()`, or as a whole within a limit with `into_vec`,
//! `into_string` and `into_json`.
//!
//! The reader carries the connection the body comes over: it stays out of the pool
//! while the body is read, goes back once the body ends, and is closed when the reader
//! is dropped before that.
#![cfg(not(target_arch = "wasm32"))]

use std::convert::Infallible;
use std::io::Write;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use flurl::{
    FlUrl, FlUrlBodyPiece, FlUrlBodyReader, FlUrlError, FlUrlHttpConnectionsCache, FlUrlMode,
    FlUrlResponse,
};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Frame;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const MODES: [FlUrlMode; 3] = [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper, FlUrlMode::H2];

const BODY: &[u8] = b"hello world";

const JSON: &[u8] = br#"{"name":"John","age":42}"#;

/// How long the server waits between two pieces of a body it sends in pieces.
const PAUSE: Duration = Duration::from_millis(100);

/// A body sent piece by piece, with no size announced.
struct BodyInPieces(mpsc::Receiver<Bytes>);

impl hyper::body::Body for BodyInPieces {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.0
            .poll_recv(cx)
            .map(|piece| piece.map(|piece| Ok(Frame::data(piece))))
    }
}

/// `ends: false` is a body which goes silent after its last piece and never ends.
fn in_pieces(pieces: &'static [&'static [u8]], ends: bool) -> BoxBody<Bytes, Infallible> {
    let (sender, receiver) = mpsc::channel(1);

    tokio::spawn(async move {
        for piece in pieces {
            if sender.send(Bytes::from_static(piece)).await.is_err() {
                return;
            }
            tokio::time::sleep(PAUSE).await;
        }

        if !ends {
            sender.closed().await;
        }
    });

    BodyInPieces(receiver).boxed()
}

fn gzipped() -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(BODY).unwrap();
    encoder.finish().unwrap()
}

struct Server {
    url: String,
    mode: FlUrlMode,
    /// How many connections the server has accepted.
    connections: Arc<AtomicUsize>,
    // A pool of its own: what another test does with the shared one is not seen here.
    cache: Arc<FlUrlHttpConnectionsCache>,
}

impl Server {
    /// Serves on a free port until the test ends:
    /// * `/whole` - the body with its size announced;
    /// * `/pieces` - the body in two pieces with a pause between them, no size announced;
    /// * `/silent` - the first of those pieces and nothing after it, the body never ends;
    /// * `/gzip` - the body gzipped;
    /// * `/json` - a JSON object.
    async fn start(mode: FlUrlMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());

        let connections = Arc::new(AtomicUsize::new(0));
        let accepted = connections.clone();
        let h2 = mode.is_h2();

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                accepted.fetch_add(1, Ordering::SeqCst);

                let io = TokioIo::new(stream);
                let service = service_fn(|request: hyper::Request<hyper::body::Incoming>| async move {
                    let is_head = request.method() == hyper::Method::HEAD;

                    let response = match request.uri().path() {
                        "/whole" => hyper::Response::new(Full::new(Bytes::from_static(BODY)).boxed()),
                        "/pieces" => hyper::Response::new(in_pieces(&[b"hello ", b"world"], true)),
                        "/silent" => hyper::Response::new(in_pieces(&[b"hello "], false)),
                        "/gzip" => hyper::Response::builder()
                            .header("content-encoding", "gzip")
                            .body(Full::new(Bytes::from(gzipped())).boxed())
                            .unwrap(),
                        "/json" => hyper::Response::builder()
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from_static(JSON)).boxed())
                            .unwrap(),
                        _ => hyper::Response::builder()
                            .status(404)
                            .body(Full::new(Bytes::new()).boxed())
                            .unwrap(),
                    };

                    // The answer to a HEAD request has the head of the answer to a GET
                    // and no body. Over HTTP/1.1 hyper leaves the body out by itself,
                    // over HTTP/2 it does not.
                    let response = if is_head {
                        response.map(|_| Full::new(Bytes::new()).boxed())
                    } else {
                        response
                    };

                    Ok::<_, Infallible>(response)
                });

                if h2 {
                    // Prior-knowledge h2 (h2c): plain TCP has no ALPN.
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service)
                            .await;
                    });
                } else {
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service)
                            .await;
                    });
                }
            }
        });

        Self {
            url,
            mode,
            connections,
            cache: Arc::new(FlUrlHttpConnectionsCache::new()),
        }
    }

    fn request(&self, path: &str) -> FlUrl {
        FlUrl::new(format!("{}{}", self.url, path))
            .update_mode(self.mode)
            .set_connections_cache(self.cache.clone())
            .set_timeout(Duration::from_secs(3))
    }

    async fn get(&self, path: &str) -> FlUrlResponse {
        let response = self.request(path).get().await.unwrap();
        assert_eq!(response.get_status_code(), 200, "{path} {:?}", self.mode);
        response
    }

    async fn get_body(&self, path: &str) -> FlUrlBodyReader {
        self.get(path).await.get_body().unwrap()
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn the_body_is_read_piece_by_piece() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut reader = server.get_body("/pieces").await;
        assert_eq!(reader.content_length(), None, "{mode:?}");

        // A piece is used and let go of: what is kept of it is a copy.
        let mut pieces = 0;
        let mut body = Vec::new();
        while let Some(piece) = reader.next_item().await.unwrap() {
            pieces += 1;
            body.extend_from_slice(piece);
        }

        // The second piece is sent after the head has arrived, and it still comes.
        assert_eq!(pieces, 2, "{mode:?}");
        assert_eq!(body, BODY, "{mode:?}");
    }
}

/// A reader may keep the piece it has while it asks for the next one, wherever the reads
/// cut the body. Here the size of the third chunk comes in a read of its own and its
/// CRLF in the read after it. A piece of `next_item` is good until the next call, so a
/// reader which keeps one reads the body as a stream of bytes.
#[tokio::test]
async fn a_reader_which_keeps_the_piece_it_has_gets_the_next_one() {
    use flurl::rust_extensions::AsyncBytesStream;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const PARTS: [&[u8]; 4] = [
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n",
        b"6\r\n world\r\n",
        b"1",
        b"\r\n!\r\n0\r\n\r\n",
    ];

    for mode in [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/", listener.local_addr().unwrap().port());

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            socket.set_nodelay(true).unwrap();

            let (read_half, mut write_half) = socket.into_split();
            let mut reader = BufReader::new(read_half);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }

            // A part at a time, so that each of them is a read of its own
            for part in PARTS {
                write_half.write_all(part).await.unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        let mut response = FlUrl::new(url)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(3))
            .get()
            .await
            .unwrap();
        let reader = response.get_body().unwrap();

        let read = tokio::time::timeout(Duration::from_secs(3), async {
            let mut body = Vec::new();
            let mut kept = None;

            while let Some(piece) = reader.get_next().await.unwrap() {
                body.extend_from_slice(&piece);

                // The piece before is let go of once this one is here
                kept = Some(piece);
            }

            drop(kept);
            body
        })
        .await;

        assert_eq!(
            read.unwrap_or_else(|_| panic!("{mode:?}: the reader which keeps a piece waits for ever")),
            b"hello world!",
            "{mode:?}"
        );
    }
}

#[tokio::test]
async fn into_vec_reads_the_whole_body() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let reader = server.get_body("/whole").await;
        assert_eq!(reader.content_length(), Some(BODY.len()), "{mode:?}");
        assert_eq!(reader.remains_to_read(), Some(BODY.len()), "{mode:?}");
        assert_eq!(reader.into_vec(BODY.len()).await.unwrap(), BODY, "{mode:?}");

        let reader = server.get_body("/pieces").await;
        assert_eq!(reader.into_vec(BODY.len()).await.unwrap(), BODY, "{mode:?}");
    }
}

#[tokio::test]
async fn into_string_and_into_json_read_the_whole_body() {
    #[derive(serde::Deserialize)]
    struct User {
        name: String,
        age: u8,
    }

    for mode in MODES {
        let server = Server::start(mode).await;

        let body = server.get_body("/whole").await.into_string(usize::MAX).await;
        assert_eq!(body.unwrap(), "hello world", "{mode:?}");

        let user: User = server.get_body("/json").await.into_json(1024).await.unwrap();
        assert_eq!(user.name, "John", "{mode:?}");
        assert_eq!(user.age, 42, "{mode:?}");

        let not_json = server.get_body("/whole").await.into_json::<User>(1024).await;
        assert!(
            matches!(not_json, Err(FlUrlError::SerializationError(_))),
            "{mode:?}"
        );
    }
}

#[tokio::test]
async fn into_vec_refuses_a_body_over_its_limit() {
    for mode in MODES {
        let server = Server::start(mode).await;
        let limit = BODY.len() - 1;

        // The size is announced: refused before a byte of the body is read.
        let body = server.get_body("/whole").await.into_vec(limit).await;
        assert!(
            matches!(body, Err(FlUrlError::ResponseBodyTooLarge { limit: reported }) if reported == limit),
            "{mode:?}: {body:?}"
        );

        // It is not: cut off once the body grows past the limit.
        let body = server.get_body("/pieces").await.into_vec(limit).await;
        assert!(
            matches!(body, Err(FlUrlError::ResponseBodyTooLarge { limit: reported }) if reported == limit),
            "{mode:?}: {body:?}"
        );

        // The same limit over a body read as a string and as JSON.
        let body = server.get_body("/whole").await.into_string(limit).await;
        assert!(
            matches!(body, Err(FlUrlError::ResponseBodyTooLarge { .. })),
            "{mode:?}: {body:?}"
        );
    }
}

/// The body is taken out of the response, and the response keeps its head.
#[tokio::test]
async fn the_response_keeps_its_head_once_the_body_is_taken() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut response = server.get("/json").await;
        let body = response.get_body().unwrap();

        assert_eq!(response.get_status_code(), 200, "{mode:?}");
        assert_eq!(
            response.get_header("content-type").unwrap(),
            Some("application/json"),
            "{mode:?}"
        );
        assert_eq!(body.into_vec(usize::MAX).await.unwrap(), JSON, "{mode:?}");

        // It is there to be taken once.
        assert!(
            matches!(response.get_body(), Err(FlUrlError::ReadingHyperBodyError(_))),
            "{mode:?}"
        );
    }
}

/// `accept_gzip` is the reader's to carry out: it gives the body decoded, however it
/// is read. Without it the body comes as it was sent.
#[tokio::test]
async fn a_gzip_body_is_decoded_when_gzip_was_accepted() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut response = server.request("/gzip").accept_gzip().get().await.unwrap();
        let body = response.get_body().unwrap();
        assert_eq!(body.content_length(), None, "{mode:?}");
        assert_eq!(body.into_vec(BODY.len()).await.unwrap(), BODY, "{mode:?}");

        let mut response = server.request("/gzip").accept_gzip().get().await.unwrap();
        let mut reader = response.get_body().unwrap();
        let mut body = Vec::new();
        while let Some(piece) = reader.next_item().await.unwrap() {
            body.extend_from_slice(piece);
        }
        assert_eq!(body, BODY, "{mode:?}");

        // The answer to a HEAD request has the header and no body to decode.
        let mut response = server.request("/gzip").accept_gzip().head().await.unwrap();
        assert_eq!(
            response.get_header("content-encoding").unwrap(),
            Some("gzip"),
            "{mode:?}"
        );
        let body = response.get_body().unwrap().into_vec(usize::MAX).await;
        assert!(
            body.as_ref().is_ok_and(|body| body.is_empty()),
            "{mode:?}: {body:?}"
        );

        let body = server.get_body("/gzip").await.into_vec(usize::MAX).await;
        assert_eq!(body.unwrap(), gzipped(), "{mode:?}");
    }
}

#[tokio::test]
async fn a_body_read_to_its_end_returns_the_connection() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut reader = server.get_body("/pieces").await;
        while reader.next_item().await.unwrap().is_some() {}

        let reader = server.get_body("/whole").await;
        assert_eq!(reader.into_vec(usize::MAX).await.unwrap(), BODY, "{mode:?}");

        let body = server.get_body("/json").await.into_json::<serde_json::Value>(1024);
        assert_eq!(body.await.unwrap()["age"], 42, "{mode:?}");

        assert_eq!(server.get_body("/whole").await.into_vec(64).await.unwrap(), BODY);

        assert_eq!(server.connections(), 1, "{mode:?}");
    }
}

#[tokio::test]
async fn a_body_being_read_keeps_its_connection_to_itself() {
    for mode in [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper] {
        let server = Server::start(mode).await;

        let mut reader = server.get_body("/pieces").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ", "{mode:?}");

        // The body is half way: a request made meanwhile goes over a connection of its
        // own, and does not wait behind the body.
        let body = server.get_body("/whole").await.into_vec(usize::MAX).await;
        assert_eq!(body.unwrap(), BODY, "{mode:?}");
        assert_eq!(server.connections(), 2, "{mode:?}");

        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"world", "{mode:?}");
        assert!(reader.next_item().await.unwrap().is_none(), "{mode:?}");
    }
}

#[tokio::test]
async fn a_reader_dropped_before_the_end_closes_the_connection() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut reader = server.get_body("/silent").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ", "{mode:?}");
        drop(reader);

        let body = server.get_body("/whole").await.into_vec(usize::MAX).await;
        assert_eq!(body.unwrap(), BODY, "{mode:?}");

        // An HTTP/2 connection is shared, and only the stream of the body is dropped.
        let expected = if mode.is_h2() { 1 } else { 2 };
        assert_eq!(server.connections(), expected, "{mode:?}");
    }
}

/// So does a response which is dropped with its body in it, and a body over the limit
/// it was read with.
#[tokio::test]
async fn a_body_which_is_not_read_closes_the_connection() {
    for mode in [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper] {
        let server = Server::start(mode).await;

        drop(server.get("/pieces").await);
        let body = server.get_body("/pieces").await.into_vec(1).await;
        assert!(body.is_err(), "{mode:?}");

        let body = server.get_body("/whole").await.into_vec(usize::MAX).await;
        assert_eq!(body.unwrap(), BODY, "{mode:?}");

        assert_eq!(server.connections(), 3, "{mode:?}");
    }
}

#[tokio::test]
async fn a_silent_body_fails_once_the_response_body_timeout_runs_out() {
    for mode in MODES {
        let server = Server::start(mode).await;

        let mut response = server
            .request("/silent")
            .set_response_body_timeout(Duration::from_millis(200))
            .get()
            .await
            .unwrap();

        let mut reader = response.get_body().unwrap();
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ", "{mode:?}");

        let started = std::time::Instant::now();
        let piece = reader.next_item().await;

        assert!(matches!(piece, Err(FlUrlError::Timeout)), "{mode:?}: {piece:?}");
        assert!(started.elapsed() < Duration::from_secs(2), "{mode:?}");
        // A body which is cut short keeps answering with the reason.
        assert!(matches!(reader.next_item().await, Err(FlUrlError::Timeout)), "{mode:?}");
    }
}

/// `set_timeout` covers the request, its head and the body read into memory after it:
/// the clock of the request does not start over for `into_vec`. A body read piece by
/// piece is not bound by it.
#[tokio::test]
async fn a_body_read_as_a_whole_is_held_to_the_timeout_of_the_request() {
    for mode in MODES {
        let server = Server::start(mode).await;
        let request = || server.request("/silent").set_timeout(Duration::from_millis(500));

        let mut response = request().get().await.unwrap();

        let started = std::time::Instant::now();
        let body = response.get_body().unwrap().into_vec(usize::MAX).await;

        assert!(matches!(body, Err(FlUrlError::Timeout)), "{mode:?}: {body:?}");
        assert!(started.elapsed() < Duration::from_secs(2), "{mode:?}");

        let mut response = request().get().await.unwrap();
        let mut reader = response.get_body().unwrap();
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ", "{mode:?}");

        // Well past the timeout of the request, and the body is still being waited for.
        let piece = tokio::time::timeout(Duration::from_millis(800), reader.next_item()).await;
        assert!(piece.is_err(), "{mode:?}: {piece:?}");
    }
}

/// The reader is an `AsyncBytesStream`: whatever reads a stream of bytes reads the body
/// with it, and gets it with all there is to do to it done — decoded.
#[tokio::test]
async fn the_body_is_read_in_chunks_as_a_bytes_stream() {
    use flurl::rust_extensions::AsyncBytesStream;

    type BytesStream = dyn AsyncBytesStream<FlUrlError, Chunk = FlUrlBodyPiece> + Send + Sync;

    async fn put_together(
        src: &(impl AsyncBytesStream<FlUrlError> + ?Sized),
    ) -> Result<Vec<u8>, FlUrlError> {
        let mut result = Vec::new();

        while let Some(chunk) = src.get_next().await? {
            result.extend_from_slice(&chunk);
        }

        Ok(result)
    }

    for mode in MODES {
        let server = Server::start(mode).await;

        let body = server.get_body("/pieces").await;
        assert_eq!(put_together(&body).await.unwrap(), BODY, "{mode:?}");

        let mut response = server.request("/gzip").accept_gzip().get().await.unwrap();
        let body: Box<BytesStream> = Box::new(response.get_body().unwrap());
        assert_eq!(put_together(body.as_ref()).await.unwrap(), BODY, "{mode:?}");

        // The `into_vec()` the trait comes with.
        let body: std::sync::Arc<BytesStream> = std::sync::Arc::new(server.get_body("/whole").await);
        assert_eq!(body.get_size(), Some(BODY.len()), "{mode:?}");
        assert_eq!(body.into_vec().await.unwrap(), BODY, "{mode:?}");

        // Every body was read to the end, so every one left its connection in the pool.
        assert_eq!(server.connections(), 1, "{mode:?}");
    }
}

/// The response as hyper's carries the body as it came, and its connection with it.
#[tokio::test]
async fn hyper_s_response_gives_the_body_and_returns_the_connection() {
    for mode in MODES {
        let server = Server::start(mode).await;

        for _ in 0..2 {
            let response = server.get("/gzip").await.into_hyper_response();
            assert_eq!(response.status(), 200, "{mode:?}");
            assert_eq!(response.headers()["content-encoding"], "gzip", "{mode:?}");

            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body.as_ref(), gzipped(), "{mode:?}");

            // The way back to the pool is taken by a task of its own.
            tokio::task::yield_now().await;
        }

        assert_eq!(server.connections(), 1, "{mode:?}");

        // Once the body is taken there is none to hand on.
        let mut response = server.get("/whole").await;
        let _body = response.get_body().unwrap();
        let body = response.into_hyper_response().into_body().collect().await;
        assert!(body.is_err(), "{mode:?}");
    }
}
