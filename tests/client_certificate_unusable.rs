//! A client certificate that can not be used for the request it is given to: one whose
//! private key can not be loaded, and a second one for the same request.
//!
//! A certificate is read from a file that settings name, so a broken one is a broken
//! setting. Its key used to be loaded in the middle of the first connection attempt,
//! where my-tls `0.1.5` unwraps it — a panic for every request made with it. Now it is
//! checked when it is given, by my-tls' own loader, and the request fails with
//! `FlUrlError::RequestBuild` before a socket is opened. A second certificate used to
//! be a panic as well.
//!
//! `with_client_certificate` exists only with a TLS provider feature, which CI does
//! not test.
#![cfg(all(not(target_arch = "wasm32"), feature = "_tls"))]

use std::time::{Duration, Instant};

use flurl::my_tls::tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use flurl::my_tls::ClientCertificate;
use flurl::{FlUrl, FlUrlError};
use tokio::net::TcpListener;

/// A key no provider can load.
fn broken_certificate() -> ClientCertificate {
    ClientCertificate {
        private_key: PrivateKeyDer::Pkcs8(vec![0u8; 16].into()),
        cert_chain: vec![CertificateDer::from(vec![0u8; 16])],
    }
}

/// A key and a certificate my-tls makes itself, so both are what a provider loads.
#[cfg(any(feature = "with-ring-tls", feature = "with-rust-tls"))]
fn working_certificate() -> ClientCertificate {
    let generated = flurl::my_tls::self_signed_cert::generate("client").unwrap();

    ClientCertificate {
        private_key: generated.private_key,
        cert_chain: vec![generated.certificate],
    }
}

/// A port where a connection would be accepted — so a request that is refused before a
/// socket is opened is told apart from one that only failed to connect.
async fn listening_url() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://127.0.0.1:{}/path", listener.local_addr().unwrap().port());
    (listener, url)
}

async fn assert_nothing_connected(listener: &TcpListener) {
    let accepted = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
    assert!(accepted.is_err(), "no connection must be opened");
}

#[tokio::test]
async fn a_key_that_can_not_be_loaded_fails_the_request() {
    let (listener, url) = listening_url().await;

    let fl_url = FlUrl::new(url.as_str()).with_client_certificate(broken_certificate());

    match fl_url.get_error() {
        Some(FlUrlError::RequestBuild(message)) => assert!(
            message.starts_with("Client certificate can not be used: "),
            "{message}"
        ),
        other => panic!("unexpected state {other:?}"),
    }

    let started = Instant::now();

    let result = fl_url
        .with_retry(Duration::from_secs(5), 3)
        .get()
        .await;

    assert!(
        matches!(result, Err(FlUrlError::RequestBuild(_))),
        "{result:?}"
    );
    // A broken setting is not an outage: nothing is waited out.
    assert!(started.elapsed() < Duration::from_secs(5));

    assert_nothing_connected(&listener).await;
}

/// The other direction: a certificate a provider loads passes the check, and the
/// request goes as far as the TLS handshake — which the plain socket here fails.
#[cfg(any(feature = "with-ring-tls", feature = "with-rust-tls"))]
#[tokio::test]
async fn a_certificate_that_can_be_loaded_reaches_the_handshake() {
    let (listener, url) = listening_url().await;

    tokio::spawn(async move {
        // Takes the connection and drops it: no handshake comes back.
        let _ = listener.accept().await;
    });

    let fl_url = FlUrl::new(url.as_str())
        .with_client_certificate(working_certificate())
        .set_timeout(Duration::from_secs(3));

    assert!(fl_url.get_error().is_none(), "{:?}", fl_url.get_error());

    match fl_url.get().await {
        Err(FlUrlError::RequestBuild(message)) => panic!("refused: {message}"),
        Err(_) => {}
        Ok(_) => panic!("there is no TLS server behind the socket"),
    }
}

#[cfg(any(feature = "with-ring-tls", feature = "with-rust-tls"))]
#[tokio::test]
async fn a_second_certificate_fails_the_request() {
    let (listener, url) = listening_url().await;

    let fl_url = FlUrl::new(url.as_str())
        .with_client_certificate(working_certificate())
        .with_client_certificate(working_certificate());

    match fl_url.get().await {
        Err(FlUrlError::RequestBuild(message)) => {
            assert_eq!(message, "Client certificate is already set")
        }
        other => panic!("unexpected result {other:?}"),
    }

    assert_nothing_connected(&listener).await;
}
