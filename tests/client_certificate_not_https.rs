//! `with_client_certificate` on a url that is not `https://`.
//!
//! Whether the url is `https://` is decided by settings as often as by code: the
//! code attaches the certificate, the settings name the address. `http://` there
//! used to be a panic inside `with_client_certificate`. It returns the builder and
//! can not report anything, so now it is the request that fails — with
//! `FlUrlError::RequestBuild`, before a socket is opened.
//!
//! `with_client_certificate` exists only with a TLS provider feature, which CI does
//! not test.
#![cfg(all(not(target_arch = "wasm32"), feature = "_tls"))]

use std::time::Duration;

use bytes::Bytes;
use flurl::body::HttpRequestBody;
use flurl::my_tls::tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use flurl::my_tls::ClientCertificate;
use flurl::{EmptyRequestModel, FlUrl, FlUrlError, FlUrlMode, FlUrlResponse, HttpVerb};
use http_body_util::Full;
use tokio::net::TcpListener;

// Nothing listens here, and nothing has to: the request is refused before a socket
// is opened.
const URL: &str = "http://127.0.0.1:1/path";

/// A key no provider can load. The url is checked first, so on the `http://` urls
/// below it is never looked into.
fn certificate() -> ClientCertificate {
    ClientCertificate {
        private_key: PrivateKeyDer::Pkcs8(vec![0u8; 16].into()),
        cert_chain: vec![CertificateDer::from(vec![0u8; 16])],
    }
}

fn fl_url(url: &str) -> FlUrl {
    FlUrl::new(url).with_client_certificate(certificate())
}

fn assert_refused(result: Result<FlUrlResponse, FlUrlError>, url: &str, case: &str) {
    match result {
        Err(FlUrlError::RequestBuild(message)) => {
            assert!(
                message.contains("Client certificate can only be used with https")
                    && message.contains(url),
                "{case}: {message}"
            );
        }
        Err(err) => panic!("{case}: unexpected error {err:?}"),
        Ok(_) => panic!("{case}: the request must be refused"),
    }
}

#[tokio::test]
async fn an_http_request_is_refused_in_every_mode() {
    for mode in [
        FlUrlMode::Http1Hyper,
        FlUrlMode::Http1NoHyper,
        FlUrlMode::H2,
    ] {
        let result = fl_url(URL).update_mode(mode).get().await;
        assert_refused(result, URL, &format!("{mode:?}"));
    }
}

#[tokio::test]
async fn an_http_request_with_no_connection_reuse_is_refused() {
    let result = fl_url(URL).do_not_reuse_connection().get().await;
    assert_refused(result, URL, "no reuse");
}

#[tokio::test]
async fn every_way_of_sending_is_refused() {
    let body = || HttpRequestBody::from_raw_data(b"body".to_vec(), None);

    assert_refused(fl_url(URL).head().await, URL, "head");
    assert_refused(fl_url(URL).delete().await, URL, "delete");
    assert_refused(fl_url(URL).post(body()).await, URL, "post");
    assert_refused(fl_url(URL).put(body()).await, URL, "put");
    assert_refused(fl_url(URL).patch(body()).await, URL, "patch");

    let result = fl_url(URL)
        .execute_request(HttpVerb::Get, EmptyRequestModel)
        .await;
    assert_refused(result, URL, "a request model");

    let result = fl_url(URL)
        .post_request_streamed(Full::new(Bytes::from_static(b"body")), None)
        .await;
    assert_refused(result, URL, "a streamed body");
}

#[tokio::test]
async fn retries_do_not_wait_on_it() {
    let started = std::time::Instant::now();

    let result = fl_url(URL)
        .with_retry(Duration::from_secs(5), 3)
        .get()
        .await;

    assert_refused(result, URL, "with_retry");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn nothing_is_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/path", listener.local_addr().unwrap().port());

    let result = fl_url(url.as_str()).get().await;
    assert_refused(result, url.as_str(), "a listening port");

    let accepted = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
    assert!(accepted.is_err(), "no connection must be opened");
}

#[cfg(unix)]
#[tokio::test]
async fn a_unix_socket_request_is_refused() {
    let url = "http+unix:///tmp/fl-url-no-such.sock";

    let result = fl_url(url).append_path_segment("path").get().await;
    assert_refused(result, "/tmp/fl-url-no-such.sock", "unix socket");
}

/// The certificate would have to reach the server behind the tunnel, and the tunnel
/// is not what it is checked against: the url behind it is.
#[cfg(all(unix, feature = "with-ssh"))]
#[tokio::test]
async fn an_http_request_through_an_ssh_tunnel_is_refused() {
    let result = fl_url("ssh://user@localhost:1->http://127.0.0.1:1/path")
        .get()
        .await;

    assert_refused(result, URL, "ssh tunnel");
}

#[test]
fn the_builder_does_not_panic() {
    for url in [URL, "127.0.0.1:1", "ws://127.0.0.1:1", "https://127.0.0.1:1"] {
        let _ = FlUrl::new(url).with_client_certificate(certificate());
    }
}
