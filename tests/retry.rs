//! `with_retry` against a real server that is being restarted: it answers `503` a
//! couple of times before it answers `200`, the way a reverse proxy answers while the
//! container behind it is being recreated — and, for the transport face of the same
//! outage, a port nobody listens on until the service comes back.
//!
//! Every `FlUrlMode` goes through the same retry loop but tears a discarded response
//! down its own way — an HTTP/1 connection is disposed, an h2 stream is reset while
//! the shared connection stays — so the main case runs in all three.
#![cfg(not(target_arch = "wasm32"))]

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use flurl::body::HttpRequestBody;
use flurl::{FlUrl, FlUrlMode};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Frame;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

const RETRY_DELAY: Duration = Duration::from_millis(300);

/// What the server answers to its n-th request. Past the end of the script it
/// answers `200`.
#[derive(Clone, Copy)]
enum Answer {
    /// A complete response, with its own status code as the body.
    Complete(u16),
    /// A response whose body never ends. A client that tried to read it before moving
    /// on would hang — only one that aborts it gets to the next attempt.
    Endless(u16),
}

#[derive(Clone, Copy)]
enum Proto {
    Http1,
    H2c,
}

/// Everything the server saw, for the test to assert on.
struct Seen {
    script: Vec<Answer>,
    connections: AtomicUsize,
    /// Method and arrival time of every request, in order.
    requests: Mutex<Vec<(hyper::Method, Instant)>>,
    /// When the server gave up on an endless body, by the index of its request. That
    /// happens only once the client closed the connection (HTTP/1) or reset the stream
    /// (h2) the body was going out on.
    aborted: Mutex<Vec<(usize, Instant)>>,
}

impl Seen {
    fn new(script: Vec<Answer>) -> Arc<Self> {
        Arc::new(Self {
            script,
            connections: AtomicUsize::new(0),
            requests: Mutex::new(Vec::new()),
            aborted: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<(hyper::Method, Instant)> {
        self.requests.lock().unwrap().clone()
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn aborted_at(&self, request_index: usize) -> Option<Instant> {
        self.aborted
            .lock()
            .unwrap()
            .iter()
            .find(|(index, _)| *index == request_index)
            .map(|(_, at)| *at)
    }
}

/// Binds a free port and serves `script` on it until the test ends.
async fn start_server(script: Vec<Answer>, proto: Proto) -> (u16, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let seen = Seen::new(script);
    tokio::spawn(serve(listener, proto, seen.clone()));

    (port, seen)
}

async fn serve(listener: TcpListener, proto: Proto, seen: Arc<Seen>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        seen.connections.fetch_add(1, Ordering::SeqCst);

        let io = TokioIo::new(stream);
        let connection_seen = seen.clone();
        let service = service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
            answer(connection_seen.clone(), request)
        });

        match proto {
            Proto::Http1 => {
                tokio::spawn(async move {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await;
                });
            }
            // Prior-knowledge h2 (h2c): plain TCP has no ALPN, so both sides just agree
            // the connection is HTTP/2.
            Proto::H2c => {
                tokio::spawn(async move {
                    let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                        .serve_connection(io, service)
                        .await;
                });
            }
        }
    }
}

async fn answer(
    seen: Arc<Seen>,
    request: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<UnsyncBoxBody<Bytes, Infallible>>, Infallible> {
    let index = {
        let mut requests = seen.requests.lock().unwrap();
        requests.push((request.method().clone(), Instant::now()));
        requests.len() - 1
    };

    let (status, body) = match seen.script.get(index).copied() {
        Some(Answer::Endless(status)) => (status, EndlessBody::new(seen.clone(), index).boxed_unsync()),
        Some(Answer::Complete(status)) => (status, Full::new(Bytes::from(status.to_string())).boxed_unsync()),
        None => (200, Full::new(Bytes::from("200")).boxed_unsync()),
    };

    Ok(hyper::Response::builder().status(status).body(body).unwrap())
}

/// A body that sends a line every few milliseconds and never ends. hyper drops it only
/// once the client is gone — the connection closed (HTTP/1) or the stream reset (h2) —
/// and the drop notes the moment.
struct EndlessBody {
    tick: tokio::time::Interval,
    seen: Arc<Seen>,
    request_index: usize,
}

impl EndlessBody {
    fn new(seen: Arc<Seen>, request_index: usize) -> Self {
        Self {
            tick: tokio::time::interval(Duration::from_millis(10)),
            seen,
            request_index,
        }
    }
}

impl hyper::body::Body for EndlessBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.tick.poll_tick(cx) {
            Poll::Ready(_) => Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(
                b"still restarting\n",
            ))))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for EndlessBody {
    fn drop(&mut self) {
        self.seen
            .aborted
            .lock()
            .unwrap()
            .push((self.request_index, Instant::now()));
    }
}

fn url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

/// A port nothing listens on: every connect to it is refused, the way it is while the
/// container that owns it is being recreated.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// `503` twice, then `200`. Returns how many connections the server accepted, which is
/// where the modes differ.
async fn a_restarting_server_is_ridden_out(mode: FlUrlMode, proto: Proto) -> usize {
    let (port, seen) = start_server(
        vec![Answer::Endless(503), Answer::Endless(503), Answer::Complete(200)],
        proto,
    )
    .await;

    let started = Instant::now();

    let mut response = FlUrl::new(url(port))
        .append_path_segment("data")
        .update_mode(mode)
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await
        .unwrap();

    let elapsed = started.elapsed();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_body_as_str().await.unwrap(), "200");

    let requests = seen.requests();
    assert_eq!(requests.len(), 3, "two 503s, then the 200");

    assert!(
        elapsed >= RETRY_DELAY * 2,
        "two pauses of {:?} took only {:?}",
        RETRY_DELAY,
        elapsed
    );

    // Neither 503 was read: the body was torn down before the next attempt went out.
    for index in 0..2 {
        let aborted_at = seen
            .aborted_at(index)
            .unwrap_or_else(|| panic!("the body of 503 #{} was never aborted", index + 1));

        assert!(
            aborted_at <= requests[index + 1].1,
            "the body of 503 #{} was still going out when the next attempt arrived",
            index + 1
        );
    }

    seen.connections()
}

#[tokio::test]
async fn http1_hyper_rides_out_a_restart() {
    let connections = a_restarting_server_is_ridden_out(FlUrlMode::Http1Hyper, Proto::Http1).await;

    // The connection a 503 came on still carries its unread body, so it is disposed:
    // every attempt dials anew.
    assert_eq!(connections, 3);
}

#[tokio::test]
async fn http1_no_hyper_rides_out_a_restart() {
    let connections =
        a_restarting_server_is_ridden_out(FlUrlMode::Http1NoHyper, Proto::Http1).await;

    assert_eq!(connections, 3);
}

#[tokio::test]
async fn h2_rides_out_a_restart_on_one_connection() {
    let connections = a_restarting_server_is_ridden_out(FlUrlMode::H2, Proto::H2c).await;

    // A 503 is an answer on a healthy connection: only its stream is reset, and the
    // shared h2 connection carries every attempt.
    assert_eq!(connections, 1);
}

/// When the attempts run out, the last one is the result — a 5xx comes back as `Ok`,
/// body and all, for the caller to read.
#[tokio::test]
async fn when_the_attempts_run_out_the_last_5xx_is_returned() {
    let (port, seen) = start_server(
        vec![
            Answer::Complete(503),
            Answer::Complete(502),
            Answer::Complete(504),
        ],
        Proto::Http1,
    )
    .await;

    let mut response = FlUrl::new(url(port))
        .with_retry(Duration::from_millis(50), 2)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 504);
    assert_eq!(response.get_body_as_str().await.unwrap(), "504");
    assert_eq!(seen.requests().len(), 3, "the first attempt and the two more it was allowed");
}

/// A POST or a PATCH that got a 5xx may still have done its work, so it goes out once
/// and the 5xx is the result.
#[tokio::test]
async fn a_post_or_a_patch_is_never_replayed() {
    for method in [hyper::Method::POST, hyper::Method::PATCH] {
        let (port, seen) = start_server(vec![Answer::Complete(503)], Proto::Http1).await;

        let fl_url = FlUrl::new(url(port)).with_retry(Duration::from_millis(50), 5);
        let body = HttpRequestBody::from_raw_data(b"{}".to_vec(), Some("application/json"));

        let response = if method == hyper::Method::POST {
            fl_url.post(body).await
        } else {
            fl_url.patch(body).await
        }
        .unwrap();

        assert_eq!(response.get_status_code(), 503);

        let requests = seen.requests();
        assert_eq!(requests.len(), 1, "{} must go out exactly once", method);
        assert_eq!(requests[0].0, method);
    }
}

/// A 4xx is the service answering, not an outage.
#[tokio::test]
async fn a_4xx_is_returned_at_once() {
    let (port, seen) = start_server(vec![Answer::Complete(404)], Proto::Http1).await;

    let response = FlUrl::new(url(port))
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 404);
    assert_eq!(seen.requests().len(), 1);
}

/// `with_retries` keeps doing what it always did: a response that arrived is the
/// result, 5xx included.
#[tokio::test]
async fn with_retries_still_returns_a_5xx_as_is() {
    let (port, seen) = start_server(vec![Answer::Complete(503)], Proto::Http1).await;

    let response = FlUrl::new(url(port)).with_retries(3).get().await.unwrap();

    assert_eq!(response.get_status_code(), 503);
    assert_eq!(seen.requests().len(), 1);
}

/// The two set one policy, so the later call is the one that counts.
#[tokio::test]
async fn the_later_of_with_retry_and_with_retries_wins() {
    let (port, seen) = start_server(vec![Answer::Complete(503)], Proto::Http1).await;

    let response = FlUrl::new(url(port))
        .with_retry(Duration::from_millis(50), 5)
        .with_retries(0)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 503);
    assert_eq!(seen.requests().len(), 1);
}

/// The transport face of a restart: while the container is gone every connect is
/// refused, and a refused connect fails in no time — without the pause the whole
/// budget would burn out long before the service is back.
#[tokio::test]
async fn a_refused_connection_is_replayed_until_the_service_is_back() {
    let port = free_port();
    let back_after = RETRY_DELAY * 2;

    let seen = Seen::new(Vec::new());
    let server_seen = seen.clone();
    tokio::spawn(async move {
        tokio::time::sleep(back_after).await;
        let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        serve(listener, Proto::Http1, server_seen).await;
    });

    let started = Instant::now();

    let mut response = FlUrl::new(url(port))
        .with_retry(RETRY_DELAY, 10)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_body_as_str().await.unwrap(), "200");
    assert!(started.elapsed() >= back_after);
    assert_eq!(seen.requests().len(), 1, "only the attempt after the comeback got through");
}

/// When the attempts run out on a transport failure, that failure is the error — and
/// every pause was still taken.
#[tokio::test]
async fn when_the_attempts_run_out_a_transport_failure_is_the_error() {
    let port = free_port();
    let delay = Duration::from_millis(100);

    let started = Instant::now();

    let result = FlUrl::new(url(port)).with_retry(delay, 3).get().await;

    assert!(result.is_err(), "nothing listens on the port");
    assert!(
        started.elapsed() >= delay * 3,
        "three pauses of {:?} took only {:?}",
        delay,
        started.elapsed()
    );
}
