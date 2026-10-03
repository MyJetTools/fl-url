//! What may stand behind an ssh tunnel: a url means there what it means without one.
//!
//! `http://`, `https://` and a unix socket are reached through the tunnel — for real
//! in `tests/ssh_tunnel_end_to_end.rs`. What fl-url does not do without a tunnel it
//! does not do through one either: `ws://` and `wss://` fail the request with the
//! same error as without a tunnel, and so does `https://` in a build with no TLS
//! provider. `ssh://…->https://…` used to go out as plain text to the TLS port; the
//! TLS session now runs inside the tunnel.
//!
//! Builds only with `with-ssh`, which CI does not test.
#![cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]

use flurl::{FlUrl, FlUrlError, FlUrlResponse};

// Nothing listens on port 1 of the ssh host: a request that tried to open the tunnel
// would fail with a connection error, not with the errors expected here.
const SSH: &str = "ssh://user@127.0.0.1:1";

#[test]
fn http_https_and_a_unix_socket_are_accepted_behind_a_tunnel() {
    for target in [
        "http://127.0.0.1:8080/api",
        "127.0.0.1:8080/api",
        "HTTP://localhost",
        "https://127.0.0.1:8443/api",
        "HTTPS://localhost",
        "http+unix:///var/run/docker.sock",
        "unix:///var/run/docker.sock",
        "/var/run/docker.sock",
        "~/docker.sock",
    ] {
        let fl_url = FlUrl::new(format!("{SSH}->{target}"));

        assert!(fl_url.get_error().is_none(), "{target}: {:?}", fl_url.get_error());
        assert!(fl_url.via_ssh(), "{target}");
    }
}

fn unsupported_scheme(result: Result<FlUrlResponse, FlUrlError>) -> String {
    match result {
        Err(FlUrlError::UnsupportedScheme(message)) => message,
        other => panic!("unexpected result {other:?}"),
    }
}

/// The errors a request to `target` fails with: with no tunnel, and through one.
async fn errors_of(target: &str) -> (String, String) {
    let direct = FlUrl::new(target).get().await;

    let through_tunnel = FlUrl::new(format!("{SSH}->{target}"))
        .set_ssh_password("password")
        .get()
        .await;

    (
        unsupported_scheme(direct),
        unsupported_scheme(through_tunnel),
    )
}

#[tokio::test]
async fn a_websocket_url_fails_as_it_does_without_a_tunnel() {
    for target in ["ws://127.0.0.1:8080/ws", "wss://127.0.0.1:8443/ws"] {
        let (direct, through_tunnel) = errors_of(target).await;

        assert!(direct.starts_with("WebSocket"), "{target}: {direct}");
        assert_eq!(through_tunnel, direct, "{target}");
    }
}

/// With no TLS provider there is nothing to run a TLS session with, inside a tunnel or
/// not.
#[cfg(not(feature = "_tls"))]
#[tokio::test]
async fn https_with_no_tls_provider_fails_as_it_does_without_a_tunnel() {
    let (direct, through_tunnel) = errors_of("https://127.0.0.1:8443/api").await;

    assert!(direct.starts_with("FlUrl does not support https"), "{direct}");
    assert_eq!(through_tunnel, direct);
}
