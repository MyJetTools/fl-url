//! `FlUrl::try_new` on a url that names no host.
//!
//! Such a url used to be accepted and then panic when a request was built from it
//! (the url builder slices the host out by offsets that an empty host turns
//! backwards). The url comes from a service's settings, so an empty value has to
//! come back as an `Err` the service can report.
#![cfg(not(target_arch = "wasm32"))]

use flurl::{FlUrl, FlUrlError};

fn assert_invalid_url(url: &str) {
    match FlUrl::try_new(url) {
        Err(FlUrlError::InvalidUrl(_)) => {}
        Err(err) => panic!("{url:?}: unexpected error {err:?}"),
        Ok(_) => panic!("{url:?} must be rejected"),
    }
}

#[test]
fn an_empty_url_is_rejected() {
    assert_invalid_url("");
    assert_invalid_url(" ");
}

#[test]
fn a_scheme_with_nothing_after_it_is_rejected() {
    for url in ["http://", "https://", "HTTP://", "HTTPS://", "ws://", "wss://"] {
        assert_invalid_url(url);
    }
}

#[test]
fn an_authority_with_no_host_in_it_is_rejected() {
    for url in [
        "http://:8080",
        "http://:8080/path",
        "http:///path",
        "http://?a=b",
        "http://#fragment",
        "http:// ",
    ] {
        assert_invalid_url(url);
    }
}

#[test]
fn a_unix_socket_url_with_no_path_is_rejected() {
    for url in ["/", "///", "http+unix:/", "http+unix://", "http+unix:///"] {
        assert_invalid_url(url);
    }
}

#[test]
fn the_error_quotes_the_url() {
    let Err(FlUrlError::InvalidUrl(message)) = FlUrl::try_new("https://") else {
        panic!("https:// must be rejected");
    };

    assert_eq!(message, "Invalid url 'https://': it names no host");
}

// The tunnel itself is fine here; it is the url behind it that names no host.
#[cfg(all(unix, feature = "with-ssh"))]
#[test]
fn a_tunnel_to_no_host_is_rejected() {
    assert_invalid_url("ssh://user@host:22->");
    assert_invalid_url("ssh://user@host:22->http://");
}

#[cfg(all(unix, feature = "with-ssh"))]
#[test]
fn a_tunnel_with_no_ssh_host_is_rejected() {
    assert_invalid_url("ssh://user@:22->http://localhost:5123");
    assert_invalid_url("ssh://user@->http://localhost:5123");
    assert_invalid_url("user@->localhost:5123");
}

#[test]
fn a_url_with_a_host_is_still_accepted() {
    for url in [
        // no scheme
        "localhost",
        "10.0.0.1:5123",
        "localhost:5123/api",
        // a path, a query
        "http://localhost",
        "http://localhost:5123/api?a=b",
        "http://localhost?a=b",
        "https://domain.com",
        "HTTP://localhost:5123",
        "http://[::1]:5123",
        "http://хост.рф/путь",
        // the connection goes to the ip, the name stays the Host header
        "http://domain.com@15.0.0.5/path",
        "domain.com@15.0.0.5:8080",
        "https://domain.com@[2001:db8::1]:8443",
    ] {
        assert!(FlUrl::try_new(url).is_ok(), "{url}");
    }
}

// The "host" of a unix socket url is the path of the socket file.
#[test]
fn a_unix_socket_url_is_still_accepted() {
    for url in [
        "/var/run/docker.sock",
        "~/docker.sock",
        "http+unix:/var/run/docker.sock",
        "http+unix://var/run/docker.sock",
        "http+unix:///var/run/docker.sock",
        "unix:///var/run/docker.sock",
        "unix://var/run/docker.sock",
        "unix+http://var/run/docker.sock",
    ] {
        assert!(FlUrl::try_new(url).is_ok(), "{url}");
    }
}
