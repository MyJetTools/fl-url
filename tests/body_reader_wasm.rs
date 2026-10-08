//! The reader of a response body on the wasm backend, in a real browser: the body
//! read piece by piece off its stream (`Response.body`), or as a whole within a limit.
//!
//! A browser test has no server to stream a body from, so `fetch` itself is swapped for
//! a stand-in whose `Response` carries a `ReadableStream` fed on a timer — and errored
//! once the request is aborted, the way the browser does it to the body of a real one.
//! Everything around it is the real thing: the stream, its reader, the
//! `AbortController`.
//!
//! Runs in headless Chrome:
//!
//! ```sh
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//! CHROMEDRIVER=/path/to/chromedriver \
//! cargo test --target wasm32-unknown-unknown --no-default-features --test body_reader_wasm
//! ```
//!
//! `wasm-bindgen-test-runner` comes with `cargo install wasm-bindgen-cli`, and it has
//! to be the exact wasm-bindgen version Cargo.lock resolved.
#![cfg(target_arch = "wasm32")]

use std::future::Future;
use std::task::{Context, Waker};
use std::time::Duration;

use flurl::{FlUrl, FlUrlBodyReader, FlUrlError};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// Absolute, so no origin gets involved; `.invalid` never resolves, and nothing tries.
const URL: &str = "https://service.invalid/api/data";

// The body the stand-in sends, its pieces separated by `|`.
const PIECES: &str = "hello |world";
const BODY: &[u8] = b"hello world";

/// How long the stand-in waits between two pieces, and after the last one before the
/// body ends.
const PAUSE_MILLIS: u32 = 50;

#[wasm_bindgen(inline_js = r#"
let original = null;
let ended = false;

// Answers every request with a 200 whose body comes in `pieces` — one string, the
// pieces separated by "|" — `pause` ms apart. `announce`: the response says the size
// of its body in Content-Length. `ends`: false leaves the body open after its last
// piece.
export function stream_fetch(pieces, pause, announce, ends) {
    const chunks = pieces.split("|");
    const encoder = new TextEncoder();
    ended = false;
    if (original === null) {
        original = globalThis.fetch;
    }
    globalThis.fetch = function (request) {
        let timer = null;
        const body = new ReadableStream({
            start(controller) {
                let index = 0;
                const send = () => {
                    if (index < chunks.length) {
                        controller.enqueue(encoder.encode(chunks[index]));
                        index += 1;
                        timer = setTimeout(send, pause);
                    } else if (ends) {
                        controller.close();
                    }
                };
                send();

                // What the browser does to the body of a request which is aborted.
                request.signal.addEventListener("abort", () => {
                    ended = true;
                    clearTimeout(timer);
                    try {
                        controller.error(
                            new DOMException("The operation was aborted.", "AbortError"));
                    } catch (_) {
                        // The body is over already.
                    }
                });
            },
        });

        const headers = new Headers();
        if (announce) {
            headers.set("content-length", String(chunks.join("").length));
        }
        return Promise.resolve(new Response(body, { status: 200, headers }));
    };
}

export function restore_fetch() {
    if (original !== null) {
        globalThis.fetch = original;
        original = null;
    }
}

// The request was aborted: the download of its body is over.
export function download_was_ended() {
    return ended;
}
"#)]
extern "C" {
    fn stream_fetch(pieces: &str, pause: u32, announce: bool, ends: bool);
    fn restore_fetch();
    fn download_was_ended() -> bool;
}

async fn get_body() -> FlUrlBodyReader {
    FlUrl::new(URL).get().await.unwrap().get_body().unwrap()
}

#[wasm_bindgen_test]
async fn the_body_is_read_piece_by_piece() {
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);

    let mut reader = get_body().await;
    assert_eq!(reader.content_length(), None);
    assert_eq!(reader.remains_to_read(), None);

    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");
    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"world");
    assert!(reader.next_item().await.unwrap().is_none());
    // A body which is over keeps being over.
    assert!(reader.next_item().await.unwrap().is_none());
    assert_eq!(reader.remains_to_read(), Some(0));

    drop(reader);
    assert!(!download_was_ended(), "a body read to its end has nothing to end");

    restore_fetch();
}

#[wasm_bindgen_test]
async fn into_vec_reads_the_whole_body() {
    // The response says its size: the body is had in one go.
    stream_fetch(PIECES, PAUSE_MILLIS, true, true);

    let reader = get_body().await;
    assert_eq!(reader.content_length(), Some(BODY.len()));
    assert_eq!(reader.remains_to_read(), Some(BODY.len()));
    assert_eq!(reader.into_vec(BODY.len()).await.unwrap(), BODY);
    assert!(!download_was_ended());

    // It does not, and there is no limit to cut it off at: in one go as well.
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);
    assert_eq!(get_body().await.into_vec(usize::MAX).await.unwrap(), BODY);

    // It does not, and there is a limit: piece by piece.
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);
    assert_eq!(get_body().await.into_vec(BODY.len()).await.unwrap(), BODY);

    // What is left of the body, once a part of it is taken.
    stream_fetch(PIECES, PAUSE_MILLIS, true, true);

    let mut reader = get_body().await;
    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");
    assert_eq!(reader.remains_to_read(), Some(5));
    assert_eq!(reader.into_vec(5).await.unwrap(), b"world");

    restore_fetch();
}

#[wasm_bindgen_test]
async fn into_string_and_into_json_read_the_whole_body() {
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);
    assert_eq!(
        get_body().await.into_string(usize::MAX).await.unwrap(),
        "hello world"
    );

    stream_fetch(r#"{"name":"John",|"age":42}"#, PAUSE_MILLIS, false, true);
    let json: serde_json::Value = get_body().await.into_json(1024).await.unwrap();
    assert_eq!(json["name"], "John");
    assert_eq!(json["age"], 42);

    stream_fetch(PIECES, PAUSE_MILLIS, false, true);
    let not_json = get_body().await.into_json::<serde_json::Value>(1024).await;
    assert!(matches!(not_json, Err(FlUrlError::SerializationError(_))));

    restore_fetch();
}

#[wasm_bindgen_test]
async fn into_vec_refuses_a_body_over_its_limit() {
    let limit = BODY.len() - 1;

    // The size is announced: refused before a byte of the body is read.
    stream_fetch(PIECES, PAUSE_MILLIS, true, true);

    let body = get_body().await.into_vec(limit).await;
    assert!(
        matches!(body, Err(FlUrlError::ResponseBodyTooLarge { limit: reported }) if reported == limit),
        "{body:?}"
    );
    assert!(download_was_ended(), "the rest of the body was left to download");

    // It is not: cut off once the body grows past the limit.
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);

    let body = get_body().await.into_vec(limit).await;
    assert!(
        matches!(body, Err(FlUrlError::ResponseBodyTooLarge { limit: reported }) if reported == limit),
        "{body:?}"
    );
    assert!(download_was_ended(), "the rest of the body was left to download");

    restore_fetch();
}

/// The body is taken out of the response, and the response keeps its head.
#[wasm_bindgen_test]
async fn the_response_keeps_its_head_once_the_body_is_taken() {
    stream_fetch(PIECES, PAUSE_MILLIS, true, true);

    let mut response = FlUrl::new(URL).get().await.unwrap();
    let body = response.get_body().unwrap();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_header("content-length").unwrap(), Some("11"));
    assert_eq!(body.into_vec(usize::MAX).await.unwrap(), BODY);

    // It is there to be taken once.
    assert!(matches!(response.get_body(), Err(FlUrlError::FetchError(_))));

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_reader_dropped_before_the_end_ends_the_download() {
    stream_fetch(PIECES, PAUSE_MILLIS, false, false);

    let mut reader = get_body().await;
    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");
    assert!(!download_was_ended());

    drop(reader);
    assert!(download_was_ended());

    // So does a response which is dropped with its body in it.
    stream_fetch(PIECES, PAUSE_MILLIS, false, false);

    let response = FlUrl::new(URL).get().await.unwrap();
    assert!(!download_was_ended());

    drop(response);
    assert!(download_was_ended());

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_silent_body_fails_once_the_response_body_timeout_runs_out() {
    let silent = || async {
        // One piece, and the body is left open after it.
        stream_fetch("hello ", PAUSE_MILLIS, false, false);

        FlUrl::new(URL)
            .set_response_body_timeout(Duration::from_millis(200))
            .get()
            .await
            .unwrap()
            .get_body()
            .unwrap()
    };

    // Piece by piece: each piece has the timeout to come in.
    let mut reader = silent().await;
    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");

    let started = js_sys::Date::now();
    let piece = reader.next_item().await;
    let elapsed = js_sys::Date::now() - started;

    assert!(matches!(piece, Err(FlUrlError::Timeout)), "{piece:?}");
    assert!(elapsed < 2000.0, "the piece was waited for {elapsed} ms");
    assert!(download_was_ended());

    // A body which is cut short keeps answering with the reason.
    let piece = reader.next_item().await;
    assert!(matches!(piece, Err(FlUrlError::Timeout)), "{piece:?}");
    assert_eq!(reader.remains_to_read(), None);

    // As a whole: the whole body has it.
    let body = silent().await.into_vec(usize::MAX).await;
    assert!(matches!(body, Err(FlUrlError::Timeout)), "{body:?}");
    assert!(download_was_ended());

    restore_fetch();
}

/// The `read()` of the stream is kept by the reader and not by the future of
/// `next_item`: the piece a dropped future was waiting for goes to the next call.
#[wasm_bindgen_test]
async fn a_read_given_up_half_way_does_not_lose_its_piece() {
    stream_fetch(PIECES, PAUSE_MILLIS, false, true);

    let mut reader = get_body().await;

    {
        let mut next = std::pin::pin!(reader.next_item());
        let mut cx = Context::from_waker(Waker::noop());
        // A promise settles in a microtask, and none has run yet.
        assert!(next.as_mut().poll(&mut cx).is_pending());
    }

    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");
    assert_eq!(reader.next_item().await.unwrap().unwrap(), b"world");
    assert!(reader.next_item().await.unwrap().is_none());

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_body_which_is_not_there_is_an_empty_one() {
    let mut reader = FlUrlBodyReader::empty();

    assert_eq!(reader.content_length(), Some(0));
    assert_eq!(reader.remains_to_read(), Some(0));
    assert!(reader.next_item().await.unwrap().is_none());
    assert!(FlUrlBodyReader::empty().into_vec(0).await.unwrap().is_empty());
}
