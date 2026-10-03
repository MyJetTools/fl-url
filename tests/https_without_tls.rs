//! An `https://` request in a build with no TLS provider feature.
//!
//! Which features a service is built with is a build-time mistake, but the service
//! learns about it at run time, on its first `https://` call — so that call has to
//! return an error, not panic.
#![cfg(all(not(target_arch = "wasm32"), not(feature = "_tls")))]

use flurl::{FlUrl, FlUrlError, FlUrlMode};

// Nothing listens here, and nothing has to: the request is refused before a socket
// is opened.
const URL: &str = "https://127.0.0.1:1/path";

async fn assert_refused(fl_url: FlUrl, case: &str) {
    match fl_url.get().await {
        Err(FlUrlError::UnsupportedScheme(message)) => {
            assert!(
                message.contains("with-ring-tls") && message.contains(URL),
                "{case}: {message}"
            );
        }
        Err(err) => panic!("{case}: unexpected error {err:?}"),
        Ok(_) => panic!("{case}: the request must be refused"),
    }
}

#[tokio::test]
async fn an_https_request_is_refused_in_every_mode() {
    for mode in [
        FlUrlMode::Http1Hyper,
        FlUrlMode::Http1NoHyper,
        FlUrlMode::H2,
    ] {
        assert_refused(FlUrl::new(URL).update_mode(mode), &format!("{mode:?}")).await;
    }
}

#[tokio::test]
async fn an_https_request_with_no_connection_reuse_is_refused() {
    assert_refused(FlUrl::new(URL).do_not_reuse_connection(), "no reuse").await;
}

#[tokio::test]
async fn the_builder_still_accepts_an_https_url() {
    // The url itself is valid: it is this build that can not serve it, and it says so
    // when the request is sent.
    assert!(FlUrl::new(URL).get_error().is_none());
}
