//! A request through an ssh tunnel that can not be opened has to fail with an error.
//!
//! Both cases below used to panic inside my-ssh on the first request, and both start
//! from a url that `FlUrl` rightly accepts:
//!
//! * the ssh host is a host name, not an ip — the form the README shows,
//!   `ssh://user@ssh.example.com:22->http://localhost:8080`. my-ssh parsed the host
//!   as an ip address and unwrapped the result;
//! * the ssh user name is longer than 255 bytes. my-ssh built `user@host:port` in a
//!   `ShortString`.
//!
//! Each kind of credentials opens the session in its own branch, so the host name
//! case runs with all three.
#![cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]

use std::time::Duration;

use flurl::FlUrl;

// `localhost` is a name, and resolving it needs no network. Nothing listens on
// port 1, so once the name is resolved the tunnel fails to open — with an error.
const HOST_NAME_URL: &str = "ssh://user@localhost:1->http://127.0.0.1:1";

fn fl_url(url: &str) -> FlUrl {
    FlUrl::new(url).set_timeout(Duration::from_secs(3))
}

#[tokio::test]
async fn a_host_name_fails_with_an_error_when_the_agent_is_used() {
    assert!(fl_url(HOST_NAME_URL).get().await.is_err());
}

#[tokio::test]
async fn a_host_name_fails_with_an_error_when_a_password_is_used() {
    let result = fl_url(HOST_NAME_URL)
        .set_ssh_password("password")
        .get()
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn a_host_name_fails_with_an_error_when_a_private_key_is_used() {
    let result = fl_url(HOST_NAME_URL)
        .set_ssh_private_key("not a key".to_string(), None)
        .get()
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn a_very_long_user_name_fails_with_an_error() {
    let url = format!("ssh://{}@127.0.0.1:1->http://127.0.0.1:1", "u".repeat(300));

    assert!(fl_url(url.as_str()).get().await.is_err());
}
