//! A `3xx` answer comes back to the caller as it is: FlUrl does not follow `Location`.
//!
//! That is the server-side behaviour on purpose — the caller sees the status and the
//! `Location` and decides itself. Under wasm the browser's `fetch` follows a redirect
//! on its own; that is the browser's doing and is not tested here.
#![cfg(not(target_arch = "wasm32"))]

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use flurl::{FlUrl, FlUrlMode};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

/// Serves on a free port until the test ends: `/start` answers `302` with
/// `Location: /other`, anything else `200`. Returns the url of `/start` and the path of
/// every request that arrived.
async fn start_server(h2: bool) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/start", listener.local_addr().unwrap().port());

    let paths = Arc::new(Mutex::new(Vec::new()));
    let server_paths = paths.clone();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            let io = TokioIo::new(stream);
            let paths = server_paths.clone();
            let service = service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
                let paths = paths.clone();
                async move {
                    let path = request.uri().path().to_string();
                    paths.lock().unwrap().push(path.clone());

                    let response = if path == "/start" {
                        hyper::Response::builder()
                            .status(302)
                            .header("location", "/other")
                            .body(Full::new(Bytes::from_static(b"moved")))
                    } else {
                        hyper::Response::builder().body(Full::new(Bytes::from_static(b"other")))
                    };

                    Ok::<_, Infallible>(response.unwrap())
                }
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

    (url, paths)
}

#[tokio::test]
async fn a_redirect_is_returned_as_it_is() {
    for mode in [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper, FlUrlMode::H2] {
        let (url, paths) = start_server(mode.is_h2()).await;

        let mut response = FlUrl::new(url)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(3))
            .get()
            .await
            .unwrap();

        assert_eq!(response.get_status_code(), 302, "{mode:?}");
        assert_eq!(
            response.get_header("location").unwrap(),
            Some("/other"),
            "{mode:?}"
        );
        assert_eq!(response.get_body().unwrap().into_vec(usize::MAX).await.unwrap(), b"moved", "{mode:?}");

        assert_eq!(*paths.lock().unwrap(), vec!["/start".to_string()], "{mode:?}");
    }
}
