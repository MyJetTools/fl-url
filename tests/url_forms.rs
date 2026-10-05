//! Forms of a url that all have to end up at the same place.
//!
//! The url is split into host, path and query by my-http-utils' `UrlBuilder`, and
//! that is what the request line, the Host header and the socket are made from — so
//! a form it splits wrongly sends a request somewhere else, or nowhere. Four of the
//! forms below used to be split wrongly, up to my-http-utils `35466bd`; each of those
//! tests says how.
#![cfg(all(unix, not(target_arch = "wasm32")))]

use bytes::Bytes;
use flurl::FlUrl;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, UnixListener};

/// What the server saw, echoed back for the test to assert on.
async fn echo(
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, std::convert::Infallible> {
    let seen = format!(
        "{}|{}|{}",
        req.method(),
        req.uri().path_and_query().map(|v| v.as_str()).unwrap_or(""),
        req.headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
    );

    Ok(hyper::Response::new(Full::new(Bytes::from(seen))))
}

async fn read_body(mut response: flurl::FlUrlResponse) -> (u16, String) {
    let status = response.get_status_code();
    let body = response.get_body_as_slice().await.unwrap().to_vec();
    (status, String::from_utf8(body).unwrap())
}

// ---- unix sockets -----------------------------------------------------------

/// Binds `path` and serves the echo above until the test ends.
async fn start_unix_server(path: &str) {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(echo))
                    .await;
            });
        }
    });
}

/// Every url in `urls` names the socket at `path`; each has to reach it, with the
/// same http path.
async fn every_form_reaches_the_socket(path: &str, urls: &[String]) {
    start_unix_server(path).await;

    for url in urls {
        let response = FlUrl::new(url.as_str())
            .append_path_segment("containers")
            .append_path_segment("json")
            .get()
            .await
            .unwrap_or_else(|err| panic!("{url}: {err:?}"));

        let (status, body) = read_body(response).await;

        assert_eq!(status, 200, "{url}");
        assert!(body.starts_with("GET|/containers/json|"), "{url}: {body}");
    }

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn the_documented_unix_socket_forms_reach_the_socket() {
    let path = "/tmp/flurl_forms_working.sock";

    every_form_reaches_the_socket(
        path,
        &[
            path.to_string(),
            // two slashes after the scheme: the path is absolute, its own slash is
            // the second one
            format!("http+unix:/{path}"),
            // three slashes: the form docs/unix-socket.md shows
            format!("http+unix://{path}"),
        ],
    )
    .await;
}

/// `UrlBuilder` used to strip only the exact prefix `http+unix:/`. Any other spelling
/// of the scheme stayed in place, so the scheme itself became the "socket path"
/// (`unix`, `unix+http`, `HTTP+UNIX`) and the real path became the http path.
#[tokio::test]
async fn every_spelling_of_the_unix_scheme_reaches_the_socket() {
    let path = "/tmp/flurl_forms_aliases.sock";

    every_form_reaches_the_socket(
        path,
        &[
            format!("unix://{path}"),
            format!("unix:/{path}"),
            format!("unix+http:/{path}"),
            format!("HTTP+UNIX:/{path}"),
        ],
    )
    .await;
}

/// One slash after the scheme: `http+unix:/var/run/x.sock` is the absolute
/// `/var/run/x.sock`. `UrlBuilder` used to read it as the relative `var/run/x.sock`,
/// which only worked when the process happened to run in `/`.
#[tokio::test]
async fn one_slash_after_the_unix_scheme_is_an_absolute_path() {
    let path = "/tmp/flurl_forms_one_slash.sock";

    every_form_reaches_the_socket(path, &[format!("http+unix:{path}"), format!("unix:{path}")])
        .await;
}

// ---- tcp ----------------------------------------------------------------------

/// Starts the echo server on a free loopback port and returns the port.
async fn start_tcp_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(echo))
                    .await;
            });
        }
    });

    port
}

#[tokio::test]
async fn a_url_in_the_query_of_a_url_with_a_scheme_is_left_alone() {
    let port = start_tcp_server().await;

    let response = FlUrl::new(format!(
        "http://127.0.0.1:{port}/api?next=http://other.invalid/x"
    ))
    .get()
    .await
    .unwrap();

    assert_eq!(
        read_body(response).await,
        (
            200,
            format!("GET|/api?next=http://other.invalid/x|127.0.0.1:{port}")
        )
    );
}

/// With no scheme of its own, the url's first `://` is the one inside the query.
/// `UrlBuilder` used to take it for the scheme separator: the host became
/// `other.invalid` and the path `/x`, while the connection still went to the real
/// host — so the request arrived there with the wrong path and the wrong Host header.
#[tokio::test]
async fn a_url_in_the_query_of_a_url_with_no_scheme_is_left_alone() {
    let port = start_tcp_server().await;

    let response = FlUrl::new(format!("127.0.0.1:{port}/api?next=http://other.invalid/x"))
        .get()
        .await
        .unwrap();

    assert_eq!(
        read_body(response).await,
        (
            200,
            format!("GET|/api?next=http://other.invalid/x|127.0.0.1:{port}")
        )
    );
}

/// The port separator is the last `:` before the path, but not one inside the
/// brackets of an IPv6 literal: those belong to the address. They used to be taken
/// for it, which cut `[::1]` down to `[:`.
#[test]
fn an_ipv6_literal_with_no_port_keeps_its_brackets() {
    for (url, host_port) in [
        ("http://[::1]", "[::1]"),
        ("http://[::1]/path", "[::1]"),
        ("http://[2001:db8::1]/path?a=b", "[2001:db8::1]"),
    ] {
        let fl_url = FlUrl::new(url);
        let url_builder = fl_url.get_url_builder().unwrap();

        assert_eq!(url_builder.get_host(), host_port, "{url}");
        assert_eq!(url_builder.get_host_port(), host_port, "{url}");
    }
}

#[test]
fn an_ipv6_literal_with_a_port_is_split_at_the_port() {
    let fl_url = FlUrl::new("http://[::1]:8080/path");
    let url_builder = fl_url.get_url_builder().unwrap();

    assert_eq!(url_builder.get_host(), "[::1]");
    assert_eq!(url_builder.get_host_port(), "[::1]:8080");
}
