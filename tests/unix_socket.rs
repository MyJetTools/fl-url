//! Unix sockets end-to-end, against real servers on a real socket file.
//!
//! The point of the file is parity: a unix socket url is the same url in every
//! `FlUrlMode`, so what h1 reaches must be what h2 reaches. H2 is the one that can
//! silently break here, because it carries no request line — the target lives in the
//! `:authority` pseudo header, which a socket path is not a legal value for.
#![cfg(all(unix, not(target_arch = "wasm32")))]

use bytes::Bytes;
use flurl::{FlUrl, FlUrlMode};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::UnixListener;

/// What the server saw, echoed back so the test can assert on it: the h2 case is
/// exactly the one where a request can be "sent" and still arrive mangled.
fn echo(req: &hyper::Request<hyper::body::Incoming>) -> String {
    format!(
        "{}|{}|{}",
        req.method(),
        req.uri().path_and_query().map(|v| v.as_str()).unwrap_or(""),
        req.headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| req.uri().authority().map(|a| a.as_str()))
            .unwrap_or("-")
    )
}

enum Proto {
    Http1,
    H2c,
}

/// Binds `path` and serves the echo above until the test ends.
async fn start_server(path: &str, proto: Proto) {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            let io = TokioIo::new(stream);

            match proto {
                Proto::Http1 => {
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service_fn(handle))
                            .await;
                    });
                }
                // Prior-knowledge h2 (h2c): there is no TLS on a unix socket, so no
                // ALPN either — both sides just agree the connection is HTTP/2.
                Proto::H2c => {
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service_fn(handle))
                            .await;
                    });
                }
            }
        }
    });

    // Give the accept loop a moment to reach the listener before the client connects.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

async fn handle(
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, std::convert::Infallible> {
    Ok(hyper::Response::new(Full::new(Bytes::from(echo(&req)))))
}

async fn read_body(mut response: flurl::FlUrlResponse) -> (u16, String) {
    let status = response.get_status_code();
    let body = response.get_body_as_slice().await.unwrap().to_vec();
    (status, String::from_utf8(body).unwrap())
}

#[tokio::test]
async fn h1_reaches_the_path_over_a_unix_socket() {
    let path = "/tmp/flurl_uds_h1.sock";
    start_server(path, Proto::Http1).await;

    let response = FlUrl::new(path)
        .append_path_segment("xxxx")
        .append_path_segment("ffff")
        .get()
        .await
        .unwrap();

    let (status, body) = read_body(response).await;

    assert_eq!(status, 200);
    assert!(body.starts_with("GET|/xxxx/ffff|"), "{}", body);

    let _ = std::fs::remove_file(path);
}

/// The regression: h2 used to fail before a byte hit the socket, with
/// `InvalidUri(InvalidUriChar)` — the socket path was being handed to `:authority`,
/// and an authority ends at its first '/'.
#[tokio::test]
async fn h2_reaches_the_same_path_over_a_unix_socket() {
    let path = "/tmp/flurl_uds_h2.sock";
    start_server(path, Proto::H2c).await;

    let response = FlUrl::new(path)
        .update_mode(FlUrlMode::H2)
        .append_path_segment("xxxx")
        .append_path_segment("ffff")
        .get()
        .await
        .unwrap();

    let (status, body) = read_body(response).await;

    assert_eq!(status, 200);
    // Same path as h1, and the placeholder authority the connector substitutes.
    assert_eq!(body, "GET|/xxxx/ffff|localhost");

    let _ = std::fs::remove_file(path);
}

/// A Host header the caller set is what the server should see — the placeholder is
/// only a fallback for when there is nothing better to send.
#[tokio::test]
async fn h2_over_a_unix_socket_keeps_a_caller_supplied_host() {
    let path = "/tmp/flurl_uds_h2_host.sock";
    start_server(path, Proto::H2c).await;

    let response = FlUrl::new(path)
        .append_path_segment("xxxx")
        .with_header("Host", "my-service")
        .update_mode(FlUrlMode::H2)
        .get()
        .await
        .unwrap();

    let (status, body) = read_body(response).await;

    assert_eq!(status, 200);
    assert_eq!(body, "GET|/xxxx|my-service");

    let _ = std::fs::remove_file(path);
}

/// Two requests in a row: the second one goes over the pooled h2 connection, which is
/// a different code path from the first (no handshake).
#[tokio::test]
async fn h2_over_a_unix_socket_reuses_the_connection() {
    let path = "/tmp/flurl_uds_h2_reuse.sock";
    start_server(path, Proto::H2c).await;

    for i in 0..3 {
        let response = FlUrl::new(path)
            .update_mode(FlUrlMode::H2)
            .append_path_segment("req")
            .append_query_param("i", Some(i.to_string().as_str()))
            .get()
            .await
            .unwrap();

        let (status, body) = read_body(response).await;

        assert_eq!(status, 200);
        assert_eq!(body, format!("GET|/req?i={}|localhost", i));
    }

    let _ = std::fs::remove_file(path);
}

/// A '~' path is accepted as a unix socket url, so it has to resolve against $HOME —
/// the OS would otherwise look for a directory literally named "~".
#[tokio::test]
async fn a_tilde_path_resolves_against_home() {
    let home = std::env::var("HOME").unwrap();
    let real_path = format!("{}/flurl_uds_tilde.sock", home);

    start_server(real_path.as_str(), Proto::Http1).await;

    let response = FlUrl::new("~/flurl_uds_tilde.sock")
        .append_path_segment("xxxx")
        .get()
        .await
        .unwrap();

    let (status, body) = read_body(response).await;

    assert_eq!(status, 200);
    assert!(body.starts_with("GET|/xxxx|"), "{}", body);

    let _ = std::fs::remove_file(real_path.as_str());
}
