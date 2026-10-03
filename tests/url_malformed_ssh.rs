//! A `FlUrl` made of a `ssh-part->target` address whose ssh part does not parse.
//!
//! The address comes from a service's settings, so a typo in it has to come back
//! as an `Err` — a panic there is a panic in every call of the http client.
#![cfg(not(target_arch = "wasm32"))]

use flurl::{FlUrl, FlUrlError};

const TARGET: &str = "http://localhost:5123";

fn assert_rejected(ssh_part: &str) {
    let url = format!("{ssh_part}->{TARGET}");

    match FlUrl::new(url.as_str()).get_error() {
        Some(FlUrlError::InvalidUrl(_)) => {}
        // Without `with-ssh` a tunnel address is refused whatever its ssh part
        // looks like.
        #[cfg(not(all(unix, feature = "with-ssh")))]
        Some(FlUrlError::UnsupportedScheme(_)) => {}
        Some(err) => panic!("{url}: unexpected error {err:?}"),
        None => panic!("{url} must be rejected"),
    }
}

#[test]
fn a_non_ssh_scheme_before_the_arrow_is_rejected() {
    // `http` typed where `ssh` was meant.
    assert_rejected("http://user@host:22");
}

#[test]
fn an_ssh_part_with_an_extra_colon_is_rejected() {
    assert_rejected("a@b:1:2");
}

#[test]
fn an_ssh_port_that_is_not_a_number_is_rejected() {
    assert_rejected("ssh://user@host:22x");
}

#[test]
fn an_ssh_port_out_of_the_u16_range_is_rejected() {
    assert_rejected("ssh://user@host:99999");
}

#[test]
fn an_empty_ssh_port_is_rejected() {
    assert_rejected("ssh://user@host:");
}

// The error of the url is what the request fails with — nothing is sent, and no ssh
// session is opened.
#[tokio::test]
async fn the_request_returns_the_error() {
    let result = FlUrl::new(format!("ssh://user@host:22x->{TARGET}"))
        .append_path_segment("api")
        .get()
        .await;

    match result {
        Err(FlUrlError::InvalidUrl(_)) => {}
        #[cfg(not(all(unix, feature = "with-ssh")))]
        Err(FlUrlError::UnsupportedScheme(_)) => {}
        Err(err) => panic!("unexpected error {err:?}"),
        Ok(_) => panic!("the request must fail"),
    }
}

// The ssh part is cut by byte offsets, so anything outside ASCII in it must not
// shift them — neither into a wrong port nor into the middle of a character.
#[cfg(all(unix, feature = "with-ssh"))]
#[test]
fn a_non_ascii_ssh_user_name_is_accepted() {
    let fl_url = FlUrl::new(format!("ssh://юзер@127.0.0.1:22->{TARGET}"));

    assert!(fl_url.get_error().is_none());
    assert!(fl_url.via_ssh());
}

#[cfg(all(unix, feature = "with-ssh"))]
#[test]
fn a_non_ascii_ssh_host_is_accepted() {
    let fl_url = FlUrl::new(format!("ssh://user@хост:22->{TARGET}"));

    assert!(fl_url.get_error().is_none());
    assert!(fl_url.via_ssh());
}

#[cfg(not(all(unix, feature = "with-ssh")))]
#[test]
fn a_non_ascii_ssh_part_does_not_panic_without_the_ssh_feature() {
    assert_rejected("ssh://юзер@127.0.0.1:22");
    assert_rejected("ssh://user@хост:22");
}
