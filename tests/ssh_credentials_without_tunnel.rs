//! The ssh credentials setters on a url that is not an ssh tunnel.
//!
//! Whether the url is a tunnel is decided by settings as often as by code: the same
//! service is given `ssh://user@host->http://…` on a developer's machine and a plain
//! `http://…` next to the server, and the code that sets the credentials is the same
//! in both. On a plain url there is nothing to apply them to, so the setters do
//! nothing and the request goes out directly. `set_ssh_password` used to panic
//! there, while `set_ssh_private_key` and `set_ssh_user_password` already were a
//! no-op.
//!
//! Builds only with `with-ssh`, which CI does not test.
#![cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]

use flurl::FlUrl;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// Accepts one connection, reads the request head, answers `200 ok`.
async fn serve_one_request(listener: TcpListener) -> String {
    let (socket, _) = listener.accept().await.unwrap();
    let (read_half, mut write_half) = socket.into_split();
    let mut reader = BufReader::new(read_half);

    let mut head = String::new();
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await.unwrap();
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
    }

    write_half
        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
        .await
        .unwrap();
    write_half.flush().await.unwrap();

    head
}

/// Sends a request through `with_credentials` to a plain `http://` url and checks
/// that it arrived — directly, since there is no tunnel to go through.
async fn the_request_goes_out_directly(with_credentials: impl FnOnce(FlUrl) -> FlUrl) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(serve_one_request(listener));

    let fl_url = with_credentials(FlUrl::new(format!("http://127.0.0.1:{}/api", port)));
    assert!(!fl_url.via_ssh());

    let mut response = fl_url.get().await.unwrap();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_body_as_slice().await.unwrap(), b"ok");

    let head = server.await.unwrap();
    assert!(head.starts_with("GET /api HTTP/1.1\r\n"), "request line: {head}");
}

#[tokio::test]
async fn set_ssh_password_does_nothing_without_a_tunnel() {
    the_request_goes_out_directly(|fl_url| fl_url.set_ssh_password("password")).await;
}

#[tokio::test]
async fn set_ssh_user_password_does_nothing_without_a_tunnel() {
    the_request_goes_out_directly(|fl_url| fl_url.set_ssh_user_password("password".to_string()))
        .await;
}

#[tokio::test]
async fn set_ssh_private_key_does_nothing_without_a_tunnel() {
    the_request_goes_out_directly(|fl_url| {
        fl_url.set_ssh_private_key("not a key".to_string(), None)
    })
    .await;
}

#[test]
fn no_form_of_a_plain_url_panics() {
    for url in [
        "http://127.0.0.1:1",
        "https://127.0.0.1:1",
        "127.0.0.1:1",
        "/tmp/fl-url-no-such.sock",
        "http+unix:///tmp/fl-url-no-such.sock",
    ] {
        let fl_url = FlUrl::new(url)
            .set_ssh_password("password")
            .set_ssh_user_password("password".to_string())
            .set_ssh_private_key("not a key".to_string(), None);

        assert!(!fl_url.via_ssh(), "{url}");
    }
}

#[test]
fn a_tunnel_url_stays_a_tunnel() {
    let fl_url = FlUrl::new("ssh://user@localhost:1->http://127.0.0.1:1")
        .set_ssh_password("password");

    assert!(fl_url.via_ssh());
}
