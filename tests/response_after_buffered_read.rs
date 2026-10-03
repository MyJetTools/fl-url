//! A response handed on — as a stream, or as hyper's response — after its body has
//! been read into memory already.
//!
//! Both used to panic once the body was loaded ("Can not get body as stream when body
//! is materialized", "Body is already disposed"). Now they give what the read loaded,
//! and after a read that failed they give the reason, as an error of the body.
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use flurl::{FlUrl, FlUrlMode, FlUrlResponse};
use http_body_util::BodyExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

// The own HTTP/1.1 implementation and hyper's read a body each in its own way.
const MODES: [FlUrlMode; 2] = [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper];

const COMPLETE: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nset-cookie: a=1\r\nset-cookie: b=2\r\nconnection: close\r\n\r\nhello";

// One chunk and no last one: the connection closes in the middle of the body. Chunked,
// because the own HTTP/1.1 implementation reads a body with a Content-Length whole
// before it hands the response over, so a short one fails the request itself.
const CUT_SHORT: &[u8] = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n";

/// Answers one request with `answer` and closes the connection.
async fn serve_one(answer: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}/data", listener.local_addr().unwrap().port());

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

        write_half.write_all(answer).await.unwrap();
        write_half.flush().await.unwrap();
    });

    url
}

async fn get(answer: &'static [u8], mode: FlUrlMode) -> FlUrlResponse {
    FlUrl::new(serve_one(answer).await)
        .update_mode(mode)
        .set_timeout(Duration::from_secs(3))
        .set_response_body_timeout(Duration::from_secs(3))
        .get()
        .await
        .unwrap()
}

#[tokio::test]
async fn the_stream_gives_the_body_that_was_read() {
    for mode in MODES {
        let mut response = get(COMPLETE, mode).await;
        assert_eq!(response.get_body_as_slice().await.unwrap(), b"hello", "{mode:?}");

        let mut stream = response.get_body_as_stream();
        assert_eq!(stream.get_parts().status, 200, "{mode:?}");

        let mut body = Vec::new();
        while let Some(chunk) = stream.get_next_chunk().await.unwrap() {
            body.extend_from_slice(&chunk);
        }

        assert_eq!(body, b"hello", "{mode:?}");
    }
}

#[tokio::test]
async fn hyper_s_response_gives_the_body_that_was_read() {
    for mode in MODES {
        let mut response = get(COMPLETE, mode).await;
        assert_eq!(response.get_body_as_str().await.unwrap(), "hello", "{mode:?}");

        let response = response.into_hyper_response();

        assert_eq!(response.status(), 200, "{mode:?}");
        assert_eq!(response.headers()["content-length"], "5", "{mode:?}");
        // Every value of a header that came more than once is still there.
        assert_eq!(
            response.headers().get_all("set-cookie").iter().count(),
            2,
            "{mode:?}"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"hello", "{mode:?}");
    }
}

#[tokio::test]
async fn after_a_failed_read_the_body_fails_with_the_reason() {
    for mode in MODES {
        let mut response = get(CUT_SHORT, mode).await;
        assert!(response.get_body_as_slice().await.is_err(), "{mode:?}");

        let mut stream = response.get_body_as_stream();
        let chunk = stream.get_next_chunk().await;
        assert!(chunk.is_err(), "{mode:?}: {chunk:?}");

        let mut response = get(CUT_SHORT, mode).await;
        assert!(response.get_body_as_slice().await.is_err(), "{mode:?}");

        let collected = response.into_hyper_response().into_body().collect().await;
        assert!(collected.is_err(), "{mode:?}");
    }
}
