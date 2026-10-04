//! A buffered read refuses a response body bigger than `set_max_response_body_size`
//! (100 MB by default) instead of holding whatever the server sends: one that announces
//! its size is refused unread, one that does not is cut off once it grows past the
//! limit, and a gzip body is measured once decoded as well. A streamed body is not
//! limited.
#![cfg(not(target_arch = "wasm32"))]

use std::io::Write;
use std::time::Duration;

use flurl::{FlUrl, FlUrlError, FlUrlMode, FlUrlResponse, DEFAULT_MAX_RESPONSE_BODY_SIZE};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

// The own HTTP/1.1 implementation and hyper's read a body each in its own way.
const MODES: [FlUrlMode; 2] = [FlUrlMode::Http1Hyper, FlUrlMode::Http1NoHyper];

/// Answers one request with `answer` and closes the connection.
async fn serve_one(answer: Vec<u8>) -> String {
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

        // The client may hang up on a body it refused; that is not the test's failure.
        let _ = write_half.write_all(&answer).await;
        let _ = write_half.flush().await;
    });

    url
}

fn with_length(body: &[u8], extra_headers: &str) -> Vec<u8> {
    let mut answer = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n{extra_headers}connection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    answer.extend_from_slice(body);
    answer
}

/// Chunked, so the size is not known until the body has been read.
fn chunked(body: &[u8]) -> Vec<u8> {
    let mut answer = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n".to_vec();
    for chunk in body.chunks(4096) {
        answer.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        answer.extend_from_slice(chunk);
        answer.extend_from_slice(b"\r\n");
    }
    answer.extend_from_slice(b"0\r\n\r\n");
    answer
}

async fn get(answer: Vec<u8>, mode: FlUrlMode, max: Option<usize>) -> FlUrlResponse {
    let mut fl_url = FlUrl::new(serve_one(answer).await)
        .update_mode(mode)
        .accept_gzip()
        .set_timeout(Duration::from_secs(5))
        .set_response_body_timeout(Duration::from_secs(5));

    if let Some(max) = max {
        fl_url = fl_url.set_max_response_body_size(max);
    }

    fl_url.get().await.unwrap()
}

fn assert_too_large(result: Result<&[u8], FlUrlError>, limit: usize, context: &str) {
    match result {
        Err(FlUrlError::ResponseBodyTooLarge { limit: reported }) => {
            assert_eq!(reported, limit, "{context}")
        }
        other => panic!("{context}: expected ResponseBodyTooLarge, got {other:?}"),
    }
}

#[tokio::test]
async fn a_body_that_announces_a_size_over_the_limit_is_refused() {
    for mode in MODES {
        let mut response = get(with_length(&[b'x'; 101], ""), mode, Some(100)).await;
        assert_too_large(response.get_body_as_slice().await, 100, &format!("{mode:?}"));
    }
}

#[tokio::test]
async fn a_chunked_body_is_cut_off_once_it_grows_past_the_limit() {
    for mode in MODES {
        let mut response = get(chunked(&[b'x'; 10_000]), mode, Some(5_000)).await;
        assert_too_large(response.get_body_as_slice().await, 5_000, &format!("{mode:?}"));
    }
}

#[tokio::test]
async fn a_body_of_exactly_the_limit_is_read() {
    for mode in MODES {
        let mut response = get(with_length(&[b'x'; 100], ""), mode, Some(100)).await;
        assert_eq!(response.get_body_as_slice().await.unwrap().len(), 100, "{mode:?}");

        let mut response = get(chunked(&[b'x'; 10_000]), mode, Some(10_000)).await;
        assert_eq!(response.get_body_as_slice().await.unwrap().len(), 10_000, "{mode:?}");
    }
}

#[tokio::test]
async fn a_gzip_body_is_measured_once_decoded() {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&[0u8; 1_000_000]).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 10_000);

    for mode in MODES {
        let answer = with_length(&compressed, "content-encoding: gzip\r\n");
        let mut response = get(answer, mode, Some(100_000)).await;
        assert_too_large(response.get_body_as_slice().await, 100_000, &format!("{mode:?}"));

        let answer = with_length(&compressed, "content-encoding: gzip\r\n");
        let mut response = get(answer, mode, Some(1_000_000)).await;
        assert_eq!(response.get_body_as_slice().await.unwrap().len(), 1_000_000, "{mode:?}");
    }
}

#[tokio::test]
async fn the_default_limit_is_a_hundred_megabytes() {
    assert_eq!(DEFAULT_MAX_RESPONSE_BODY_SIZE, 100 * 1024 * 1024);

    let body = vec![b'x'; DEFAULT_MAX_RESPONSE_BODY_SIZE + 1];
    for mode in MODES {
        let mut response = get(chunked(&body), mode, None).await;
        assert_too_large(
            response.get_body_as_slice().await,
            DEFAULT_MAX_RESPONSE_BODY_SIZE,
            &format!("{mode:?}"),
        );
    }
}

#[tokio::test]
async fn usize_max_lifts_the_limit() {
    let body = vec![b'x'; DEFAULT_MAX_RESPONSE_BODY_SIZE + 1];
    for mode in MODES {
        let response = get(chunked(&body), mode, Some(usize::MAX)).await;
        assert_eq!(response.receive_body().await.unwrap().len(), body.len(), "{mode:?}");
    }
}

#[tokio::test]
async fn a_streamed_body_is_not_limited() {
    for mode in MODES {
        let response = get(chunked(&[b'x'; 10_000]), mode, Some(100)).await;

        let mut stream = response.get_body_as_stream();
        let mut received = 0;
        while let Some(chunk) = stream.get_next_chunk().await.unwrap() {
            received += chunk.len();
        }

        assert_eq!(received, 10_000, "{mode:?}");
    }
}

/// The head announces a body over the limit and the body never comes. The read is
/// refused on the head alone. Hyper only: in Http1NoHyper my-http-client reads a
/// Content-Length body whole before it hands the response over.
#[tokio::test]
async fn a_body_over_the_limit_is_refused_without_waiting_for_it() {
    for mode in [FlUrlMode::Http1Hyper] {
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

            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n",
                DEFAULT_MAX_RESPONSE_BODY_SIZE + 1
            );
            write_half.write_all(head.as_bytes()).await.unwrap();
            // Holds the connection open with the body never sent
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        let started = std::time::Instant::now();
        let mut response = FlUrl::new(url)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(5))
            .get()
            .await
            .unwrap();

        assert_too_large(
            response.get_body_as_slice().await,
            DEFAULT_MAX_RESPONSE_BODY_SIZE,
            &format!("{mode:?}"),
        );
        assert!(started.elapsed() < Duration::from_secs(2), "{mode:?}");
    }
}

/// The `Content-Length` of a HEAD response, of a 304, is the size of a body they do
/// not carry: it is no reason to refuse reading their empty body.
#[tokio::test]
async fn a_response_without_a_body_is_not_refused_for_its_content_length() {
    let announced = "HTTP/1.1 200 OK\r\ncontent-length: 20000000\r\n\r\n";
    let not_modified = "HTTP/1.1 304 Not Modified\r\ncontent-length: 20000000\r\n\r\n";

    for mode in MODES {
        let url = serve_one(announced.as_bytes().to_vec()).await;
        let mut response = FlUrl::new(url)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(5))
            .set_max_response_body_size(100)
            .head()
            .await
            .unwrap();
        assert_eq!(response.get_body_as_slice().await.unwrap(), b"", "HEAD {mode:?}");

        let url = serve_one(not_modified.as_bytes().to_vec()).await;
        let mut response = FlUrl::new(url)
            .update_mode(mode)
            .set_timeout(Duration::from_secs(5))
            .set_max_response_body_size(100)
            .get()
            .await
            .unwrap();
        assert_eq!(response.get_status_code(), 304, "{mode:?}");
        assert_eq!(response.get_body_as_slice().await.unwrap(), b"", "304 {mode:?}");
    }
}

/// Next to `Transfer-Encoding: chunked` a `Content-Length` does not frame the body, so
/// it is no reason to refuse a small chunked body. Hyper only: my-http-client takes
/// whichever of the two headers comes last.
#[tokio::test]
async fn content_length_next_to_transfer_encoding_is_ignored() {
    let mut answer =
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\ncontent-length: 20000000\r\nconnection: close\r\n\r\n"
            .to_vec();
    answer.extend_from_slice(b"2\r\nok\r\n0\r\n\r\n");

    for mode in [FlUrlMode::Http1Hyper] {
        let mut response = get(answer.clone(), mode, Some(100)).await;
        assert_eq!(response.get_body_as_slice().await.unwrap(), b"ok", "{mode:?}");
    }
}
