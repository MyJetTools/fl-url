//! A request that carries a websocket handshake, answered with `101 Switching
//! Protocols`.
//!
//! fl-url does not do websockets, but a caller can put the handshake headers on a
//! request, and a server can answer them. Whatever the build, the answer is the 101
//! itself or an error — never a panic:
//!
//! * without my-http-client's `with-websocket` feature the 101 comes back as a
//!   response, in both HTTP/1.1 modes;
//! * with it — Cargo unifies the feature for every user of the crate once another
//!   crate in the build turns it on — the own HTTP/1.1 implementation reports
//!   `UpgradedToWebSocket`, and hyper's client `Disconnected`: the connection is
//!   retired at the upgrade before the client looks at it.
//!
//! Had hyper's client got there first, it would have handed the upgrade over, and
//! fl-url met that with `unreachable!`. It is `UpgradedToWebSocket` now; this test does
//! not reach that arm, since the client lost the race in every run here.
//!
//! The feature build runs with
//! `cargo test --test websocket_upgrade --features my-http-client/with-websocket`.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use flurl::{FlUrl, FlUrlError, FlUrlMode};
use my_http_client::MyHttpClientError;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const SWITCHING_PROTOCOLS: &[u8] = b"HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n";

/// Answers one request with a 101 and holds the connection open for a while, the way
/// a websocket server does.
async fn serve_one_upgrade() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/ws", listener.local_addr().unwrap().port());

    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);

        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).await.unwrap();
            if read == 0 || line == "\r\n" {
                break;
            }
        }

        write_half.write_all(SWITCHING_PROTOCOLS).await.unwrap();
        write_half.flush().await.unwrap();

        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    url
}

#[tokio::test]
async fn a_101_to_a_websocket_handshake_is_the_101_or_an_error() {
    for mode in [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper] {
        let result = FlUrl::new(serve_one_upgrade().await)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(3))
            .with_header("Upgrade", "websocket")
            .with_header("Connection", "Upgrade")
            .with_header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .with_header("Sec-WebSocket-Version", "13")
            .get()
            .await;

        match result {
            Ok(response) => assert_eq!(response.get_status_code(), 101, "{mode:?}"),
            Err(FlUrlError::MyHttpClientError(MyHttpClientError::UpgradedToWebSocket)) => {}
            Err(FlUrlError::MyHttpClientError(MyHttpClientError::Disconnected))
                if matches!(mode, FlUrlMode::Http1Hyper) => {}
            Err(err) => panic!("{mode:?}: unexpected error {err:?}"),
        }
    }
}
