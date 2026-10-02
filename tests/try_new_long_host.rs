//! `FlUrl::try_new` on a url whose host is too long to be a host.
//!
//! `host:port` travels through rust-extensions as a `ShortString`, which panics past
//! 255 bytes — and it did, on the first request. A value that long in a url's place
//! is something else pasted there by mistake, so it has to come back as an `Err`.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use flurl::{FlUrl, FlUrlError, FlUrlMode};

/// A host name of exactly `len` bytes.
fn host(len: usize) -> String {
    "h".repeat(len)
}

fn assert_too_long(url: String) {
    match FlUrl::try_new(url.as_str()) {
        Err(FlUrlError::InvalidUrl(message)) => {
            assert!(message.ends_with("the host is too long"), "{message}")
        }
        Err(err) => panic!("unexpected error {err:?}"),
        Ok(_) => panic!("a url of {} bytes must be rejected", url.len()),
    }
}

#[test]
fn a_host_that_does_not_fit_is_rejected() {
    assert_too_long(format!("http://{}", host(253)));
    assert_too_long(host(253));
    assert_too_long(format!("http://{}/path", host(253)));
    assert_too_long(format!("http://{}:65535", host(250)));
    assert_too_long(format!("http://{}", host(300)));
    assert_too_long(format!("https://{}", host(252)));
}

#[test]
fn a_unix_socket_path_that_does_not_fit_is_rejected() {
    assert_too_long(format!("/{}", host(255)));
    assert_too_long(format!("http+unix://{}", host(255)));
}

#[test]
fn the_longest_host_that_fits_is_accepted() {
    for url in [
        format!("http://{}", host(252)),
        host(252),
        format!("http://{}:65535", host(249)),
        format!("https://{}", host(251)),
        format!("/{}", host(254)),
    ] {
        assert!(FlUrl::try_new(url.as_str()).is_ok(), "{} bytes", url.len());
    }
}

// The edge of what is accepted has to work all the way down: a request built from it
// fails to connect, it does not panic. The `name@ip` form keeps DNS out of the test;
// the name plus the `:1` it gets is exactly 255 bytes.
#[tokio::test]
async fn a_request_to_the_longest_host_fails_with_an_error() {
    for mode in [
        FlUrlMode::Http1Hyper,
        FlUrlMode::Http1NoHyper,
        FlUrlMode::H2,
    ] {
        let result = FlUrl::new(format!("http://{}@127.0.0.1:1", host(253)))
            .update_mode(mode)
            .set_timeout(Duration::from_secs(3))
            .append_path_segment("api")
            .get()
            .await;

        assert!(result.is_err(), "{mode:?}");
    }
}
