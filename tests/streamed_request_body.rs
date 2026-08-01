//! The streamed request body methods against a real socket: what actually lands on
//! the wire under each framing, and that a body far bigger than the process would
//! tolerate in memory goes through at constant memory.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use flurl::FlUrl;
use my_http_client::RequestBodyStream;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const CHUNK_SIZE: usize = 64 * 1024;

/// What the test server saw. The body is counted, never accumulated — otherwise the
/// server side of the memory test would grow by the size of the payload and the test
/// would fail for the wrong reason.
struct ReceivedRequest {
    head: String,
    body_len: usize,
    body_is_all_x: bool,
}

impl ReceivedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_lowercase();
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.trim().to_lowercase() == name {
                Some(value.trim())
            } else {
                None
            }
        })
    }

    fn has_header(&self, name: &str) -> bool {
        self.header(name).is_some()
    }

    fn header_count(&self, name: &str) -> usize {
        let name = name.to_lowercase();
        self.head
            .lines()
            .filter(|line| match line.split_once(':') {
                Some((key, _)) => key.trim().to_lowercase() == name,
                None => false,
            })
            .count()
    }

}

/// Accepts one connection, reads the whole request, answers `200 ok`.
async fn serve_one_request(listener: TcpListener, verify_payload: bool) -> ReceivedRequest {
    let (socket, _) = listener.accept().await.unwrap();
    let (read_half, mut write_half) = socket.into_split();
    let mut reader = BufReader::new(read_half);

    let head = read_head(&mut reader).await;

    let is_chunked = head
        .lines()
        .any(|line| line.to_lowercase().starts_with("transfer-encoding:") && line.contains("chunked"));

    let (body_len, body_is_all_x) = if is_chunked {
        read_chunked_body(&mut reader, verify_payload).await
    } else {
        let content_length = head
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                if key.trim().to_lowercase() == "content-length" {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        read_fixed_body(&mut reader, content_length, verify_payload).await
    };

    write_half
        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
        .await
        .unwrap();
    write_half.flush().await.unwrap();

    ReceivedRequest {
        head,
        body_len,
        body_is_all_x,
    }
}

async fn read_head(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> String {
    let mut head = String::new();

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await.unwrap();
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
    }

    head
}

async fn read_chunked_body(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    verify_payload: bool,
) -> (usize, bool) {
    let mut total = 0usize;
    let mut all_x = true;
    let mut scratch = vec![0u8; CHUNK_SIZE];

    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line).await.unwrap();

        // A chunk size may carry extensions after a ';'
        let size_token = size_line.trim();
        let size_token = size_token.split(';').next().unwrap();
        let size = usize::from_str_radix(size_token, 16).unwrap();

        if size == 0 {
            // Trailers (none here) followed by the final CRLF
            let mut trailer = String::new();
            let _ = reader.read_line(&mut trailer).await;
            break;
        }

        let mut left = size;
        while left > 0 {
            let take = left.min(scratch.len());
            reader.read_exact(&mut scratch[..take]).await.unwrap();
            if verify_payload && scratch[..take].iter().any(|byte| *byte != b'x') {
                all_x = false;
            }
            left -= take;
            total += take;
        }

        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).await.unwrap();
    }

    (total, all_x)
}

async fn read_fixed_body(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    content_length: usize,
    verify_payload: bool,
) -> (usize, bool) {
    let mut total = 0usize;
    let mut all_x = true;
    let mut scratch = vec![0u8; CHUNK_SIZE];

    while total < content_length {
        let take = (content_length - total).min(scratch.len());
        reader.read_exact(&mut scratch[..take]).await.unwrap();
        if verify_payload && scratch[..take].iter().any(|byte| *byte != b'x') {
            all_x = false;
        }
        total += take;
    }

    (total, all_x)
}

/// Pushes `chunks` chunks of `CHUNK_SIZE` bytes and drops the publisher, which is
/// what ends the body.
fn publish_x_chunks(publisher: my_http_client::RequestBodyPublisher<Vec<u8>>, chunks: usize) {
    tokio::spawn(async move {
        for _ in 0..chunks {
            if publisher.publish(vec![b'x'; CHUNK_SIZE]).await.is_err() {
                break;
            }
        }
    });
}

#[tokio::test]
async fn streamed_post_puts_the_payload_on_the_wire_chunked() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(serve_one_request(listener, true));

    const CHUNKS: usize = 16;

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        .with_header("Content-Type", "application/octet-stream")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(30))
        .post_request_streamed(body, None)
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_body_as_slice().await.unwrap(), b"ok");

    let received = server.await.unwrap();

    assert!(
        received.head.starts_with("POST /upload HTTP/1.1"),
        "unexpected request line: {}",
        received.head
    );
    assert_eq!(
        received.header("content-type"),
        Some("application/octet-stream")
    );

    // The size of a pushed body is unknown up front, so hyper frames it chunked and
    // there is no Content-Length to contradict it.
    assert_eq!(received.header("transfer-encoding"), Some("chunked"));
    assert!(!received.has_header("content-length"));

    assert_eq!(received.body_len, CHUNKS * CHUNK_SIZE);
    assert!(received.body_is_all_x, "the payload was corrupted on the way");
}

/// Both framings read and write the payload in pieces — that is what keeps memory
/// flat. Chunked wraps every piece in its own size prefix; `Content-Length` puts the
/// bytes on the wire bare. The envelope is the only difference, so neither may grow
/// with the size of the body.
async fn stream_a_large_body_and_measure_rss(content_length: Option<usize>) -> u64 {
    // 256 MiB: enough that materializing it would be plainly visible in RSS, and far
    // past the assertion threshold below.
    const CHUNKS: usize = 4096;
    const TOTAL: usize = CHUNKS * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(serve_one_request(listener, false));

    let rss_before = rss_bytes();

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(600))
        .post_request_streamed(body, content_length)
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    let _ = response.get_body_as_slice().await.unwrap();

    let rss_after = rss_bytes();

    let received = server.await.unwrap();
    assert_eq!(received.body_len, TOTAL);

    // The framing each case was supposed to produce actually reached the wire, so a
    // silent fallback can not make one of these two measurements meaningless.
    match content_length {
        Some(len) => {
            assert_eq!(received.header("content-length"), Some(len.to_string().as_str()));
            assert!(!received.has_header("transfer-encoding"));
        }
        None => {
            assert_eq!(received.header("transfer-encoding"), Some("chunked"));
            assert!(!received.has_header("content-length"));
        }
    }

    // The publisher is allowed 4 chunks of run-ahead, the socket holds one more, so
    // the whole transfer should cost a few hundred KiB of payload buffers. 64 MiB is
    // a quarter of the body — well clear of the noise of a test process, and nowhere
    // near what buffering 256 MiB would show.
    if let (Some(before), Some(after)) = (rss_before, rss_after) {
        let growth = after.saturating_sub(before);
        println!(
            "streamed {} MiB as {}, RSS grew by {} KiB",
            TOTAL / (1024 * 1024),
            if content_length.is_some() { "content-length" } else { "chunked" },
            growth / 1024
        );
        assert!(
            growth < 64 * 1024 * 1024,
            "RSS grew by {} bytes while streaming {} bytes - the body is being buffered",
            growth,
            TOTAL
        );
    }

    TOTAL as u64
}

#[tokio::test]
async fn chunked_keeps_memory_flat_regardless_of_the_body_size() {
    stream_a_large_body_and_measure_rss(None).await;
}

#[tokio::test]
async fn content_length_keeps_memory_flat_regardless_of_the_body_size() {
    let total = stream_a_large_body_and_measure_rss(Some(4096 * CHUNK_SIZE)).await;
    assert_eq!(total, (4096 * CHUNK_SIZE) as u64);
}

#[tokio::test]
async fn compress_is_refused_for_a_streamed_body() {
    let (publisher, body) = RequestBodyStream::<Vec<u8>>::new(1);
    drop(publisher);

    let result = FlUrl::new("http://127.0.0.1:1")
        .compress()
        .post_request_streamed(body, None)
        .await;

    match result {
        Err(flurl::FlUrlError::StreamedBodyCanNotBeCompressed) => {}
        Err(err) => panic!("expected StreamedBodyCanNotBeCompressed, got {:?}", err),
        Ok(_) => panic!("compress() over a streamed body must not be silently ignored"),
    }
}

#[tokio::test]
async fn a_streamed_request_is_never_replayed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let accepts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let server_accepts = accepts.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            server_accepts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // Killed without an answer: the request dies mid-flight, which is exactly
            // the case with_retries(3) would normally replay.
            drop(socket);
        }
    });

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, 8);

    let result = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        .do_not_reuse_connection()
        .with_retries(3)
        .set_timeout(Duration::from_secs(10))
        .post_request_streamed(body, None)
        .await;

    assert!(
        result.is_err(),
        "a request against a socket that is dropped without an answer must fail"
    );

    // Give a stray retry a chance to show up before counting.
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        accepts.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the payload is consumed as it is sent, so the request must be attempted exactly once"
    );
}

/// Resident set size of this process, or `None` on a platform we cannot read it on
/// (the assertion is then skipped rather than faked).
#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages * 4096)
}

#[cfg(target_os = "macos")]
fn rss_bytes() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;

    let kilobytes: u64 = String::from_utf8_lossy(&output.stdout).trim().parse().ok()?;
    Some(kilobytes * 1024)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rss_bytes() -> Option<u64> {
    None
}

// ---- Content-Length framing -------------------------------------------------

#[tokio::test]
async fn an_explicit_content_length_replaces_the_chunked_framing() {
    // `RequestBodyStream` reports no size hint, so a streamed body is chunked by
    // default. Passing a length switches the framing: the header wins over what the
    // body says about itself, which is what endpoints that refuse a chunked request
    // body need.
    const CHUNKS: usize = 8;
    const TOTAL: usize = CHUNKS * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(serve_one_request(listener, true));

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(30))
        .post_request_streamed(body, Some(TOTAL))
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    let _ = response.get_body_as_slice().await.unwrap();

    let received = server.await.unwrap();

    assert_eq!(received.header("content-length"), Some(TOTAL.to_string().as_str()));
    assert!(
        !received.has_header("transfer-encoding"),
        "an explicit Content-Length must suppress chunked framing, head was:\n{}",
        received.head
    );
    assert_eq!(received.body_len, TOTAL);
    assert!(received.body_is_all_x, "the payload was corrupted on the way");
}

#[tokio::test]
async fn a_stream_shorter_than_the_declared_content_length_fails_the_request() {
    // The counterpart of the test above: once Content-Length is on the wire the
    // message length is fixed by the protocol, so a body that ends early must fail
    // the request rather than leave a truncated payload behind.
    const DECLARED: usize = 8 * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        // Held open and silent: the failure has to come from the framing, not from
        // the peer going away.
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(socket);
    });

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, 4); // half of what was declared

    let result = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(4))
        .post_request_streamed(body, Some(DECLARED))
        .await;

    match result {
        Ok(_) => panic!("a short body under an explicit Content-Length must not succeed"),
        Err(err) => {
            // hyper aborts the write instead of padding or truncating silently
            let rendered = format!("{:?}", err);
            assert!(
                rendered.contains("user body write aborted"),
                "expected the body write to be aborted, got {}",
                rendered
            );
        }
    }
}

#[tokio::test]
async fn streamed_put_with_a_length_uses_content_length_framing() {
    // PUT + a known length + a streamed body: an upload of something whose size is
    // known, which is the combination endpoints that refuse chunked request bodies
    // require.
    const CHUNKS: usize = 8;
    const TOTAL: usize = CHUNKS * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(serve_one_request(listener, true));

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("files")
        .append_path_segment("archive.tar")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(30))
        .put_request_streamed(body, Some(TOTAL))
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    let _ = response.get_body_as_slice().await.unwrap();

    let received = server.await.unwrap();

    assert!(
        received.head.starts_with("PUT /files/archive.tar HTTP/1.1"),
        "unexpected request line: {}",
        received.head
    );
    assert_eq!(
        received.header("content-length"),
        Some(TOTAL.to_string().as_str())
    );
    assert!(!received.has_header("transfer-encoding"));
    assert_eq!(received.body_len, TOTAL);
    assert!(received.body_is_all_x);
}

#[tokio::test]
async fn the_content_length_argument_wins_over_a_manually_added_header() {
    // Two `content-length` header lines are a protocol violation, not merely a
    // confusing header list, so the argument replaces the manual one instead of
    // being appended next to it.
    const CHUNKS: usize = 4;
    const TOTAL: usize = CHUNKS * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(serve_one_request(listener, true));

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        // stale, and deliberately wrong
        .with_header("Content-Length", "1")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(30))
        .put_request_streamed(body, Some(TOTAL))
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    let _ = response.get_body_as_slice().await.unwrap();

    let received = server.await.unwrap();

    assert_eq!(
        received.header_count("content-length"),
        1,
        "exactly one Content-Length must reach the wire, head was:\n{}",
        received.head
    );
    assert_eq!(
        received.header("content-length"),
        Some(TOTAL.to_string().as_str())
    );
    assert_eq!(received.body_len, TOTAL);
}

#[tokio::test]
async fn a_none_length_strips_a_manually_added_content_length() {
    // The argument is the single source of the framing, so `None` does not merely
    // "not add" a Content-Length — it removes one the caller left on the builder.
    // Otherwise a stale header would claim a length the stream has no idea about.
    const CHUNKS: usize = 4;
    const TOTAL: usize = CHUNKS * CHUNK_SIZE;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(serve_one_request(listener, true));

    let (publisher, body) = RequestBodyStream::new(4);
    publish_x_chunks(publisher, CHUNKS);

    let mut response = FlUrl::new(format!("http://127.0.0.1:{}", port))
        .append_path_segment("upload")
        // left over on the builder, and nothing the stream promised to honour
        .with_header("Content-Length", "999999")
        .do_not_reuse_connection()
        .set_timeout(Duration::from_secs(30))
        .post_request_streamed(body, None)
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    let _ = response.get_body_as_slice().await.unwrap();

    let received = server.await.unwrap();

    assert!(
        !received.has_header("content-length"),
        "a None length must strip the manual header, head was:\n{}",
        received.head
    );
    assert_eq!(received.header("transfer-encoding"), Some("chunked"));
    assert_eq!(received.body_len, TOTAL);
    assert!(received.body_is_all_x);
}
