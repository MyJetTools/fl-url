//! `with_retry` on the wasm backend, in a real browser. A browser test has no server
//! to restart, so `fetch` itself is swapped for a stand-in that follows a script —
//! `503` twice, then `200`, the way a reverse proxy answers while the container behind
//! it is being recreated. Everything around it is the real thing: the `Request` FlUrl
//! builds, the `setTimeout` pause, the `AbortController`.
//!
//! Runs in headless Chrome:
//!
//! ```sh
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//! CHROMEDRIVER=/path/to/chromedriver \
//! cargo test --target wasm32-unknown-unknown --no-default-features --test retry_wasm
//! ```
//!
//! `wasm-bindgen-test-runner` comes with `cargo install wasm-bindgen-cli`, and it has
//! to be the exact wasm-bindgen version Cargo.lock resolved.
#![cfg(target_arch = "wasm32")]

use std::time::Duration;

use flurl::body::HttpRequestBody;
use flurl::{FlUrl, FlUrlError};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const RETRY_DELAY: Duration = Duration::from_millis(300);

// Absolute, so no origin gets involved; `.invalid` never resolves, and nothing tries.
const URL: &str = "https://service.invalid/api/data";

// Script steps besides a status code: a `fetch` that rejects the way it does when
// nothing answers, and one that never settles until its request is aborted.
const NETWORK_FAILURE: i32 = 0;
const HANG: i32 = -1;

#[wasm_bindgen(inline_js = r#"
let calls = [];
let original = null;

// Past the end of the script the stand-in answers 200.
export function script_fetch(script) {
    const steps = Array.from(script);
    calls = [];
    if (original === null) {
        original = globalThis.fetch;
    }
    globalThis.fetch = function (request) {
        const previous = calls[calls.length - 1];
        calls.push({
            method: request.method,
            signal: request.signal,
            previous_was_aborted: previous !== undefined && previous.signal.aborted,
        });

        const step = calls.length <= steps.length ? steps[calls.length - 1] : 200;
        if (step === 0) {
            return Promise.reject(new TypeError("Failed to fetch"));
        }
        if (step < 0) {
            return new Promise((_, reject) => {
                request.signal.addEventListener("abort", () =>
                    reject(new DOMException("The operation was aborted.", "AbortError")));
            });
        }
        return Promise.resolve(new Response(String(step), { status: step }));
    };
}

export function restore_fetch() {
    if (original !== null) {
        globalThis.fetch = original;
        original = null;
    }
}

export function fetch_calls() {
    return calls.length;
}

export function fetch_method(index) {
    return calls[index].method;
}

export function aborted_before_next_attempt(index) {
    return calls[index + 1].previous_was_aborted;
}
"#)]
extern "C" {
    fn script_fetch(script: Vec<i32>);
    fn restore_fetch();
    fn fetch_calls() -> usize;
    fn fetch_method(index: usize) -> String;
    fn aborted_before_next_attempt(index: usize) -> bool;
}

fn millis(duration: Duration) -> f64 {
    duration.as_millis() as f64
}

#[wasm_bindgen_test]
async fn a_restarting_server_is_ridden_out() {
    script_fetch(vec![503, 503, 200]);

    let started = js_sys::Date::now();

    let mut response = FlUrl::new(URL)
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await
        .unwrap();

    let elapsed = js_sys::Date::now() - started;

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(response.get_body().unwrap().into_string(usize::MAX).await.unwrap(), "200");
    assert_eq!(fetch_calls(), 3, "two 503s, then the 200");

    assert!(
        elapsed >= 2.0 * millis(RETRY_DELAY),
        "two pauses of {:?} took only {} ms",
        RETRY_DELAY,
        elapsed
    );

    // Neither 503 was read: each was aborted before the next attempt went out.
    assert!(aborted_before_next_attempt(0), "the first 503 was left open");
    assert!(aborted_before_next_attempt(1), "the second 503 was left open");

    restore_fetch();
}

#[wasm_bindgen_test]
async fn when_the_attempts_run_out_the_last_5xx_is_returned() {
    script_fetch(vec![503, 502, 504]);

    let mut response = FlUrl::new(URL)
        .with_retry(Duration::from_millis(50), 2)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 504);
    assert_eq!(response.get_body().unwrap().into_string(usize::MAX).await.unwrap(), "504");
    assert_eq!(fetch_calls(), 3, "the first attempt and the two more it was allowed");

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_post_or_a_patch_is_never_replayed() {
    for method in ["POST", "PATCH"] {
        script_fetch(vec![503]);

        let fl_url = FlUrl::new(URL).with_retry(Duration::from_millis(50), 5);
        let body = HttpRequestBody::from_raw_data(b"{}".to_vec(), Some("application/json"));

        let response = if method == "POST" {
            fl_url.post(body).await
        } else {
            fl_url.patch(body).await
        }
        .unwrap();

        assert_eq!(response.get_status_code(), 503);
        assert_eq!(fetch_calls(), 1, "{} must go out exactly once", method);
        assert_eq!(fetch_method(0), method);
    }

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_4xx_is_returned_at_once() {
    script_fetch(vec![404]);

    let response = FlUrl::new(URL)
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 404);
    assert_eq!(fetch_calls(), 1);

    restore_fetch();
}

#[wasm_bindgen_test]
async fn with_retries_still_returns_a_5xx_as_is() {
    script_fetch(vec![503]);

    let response = FlUrl::new(URL).with_retries(3).get().await.unwrap();

    assert_eq!(response.get_status_code(), 503);
    assert_eq!(fetch_calls(), 1);

    restore_fetch();
}

#[wasm_bindgen_test]
async fn the_later_of_with_retry_and_with_retries_wins() {
    script_fetch(vec![503]);

    let response = FlUrl::new(URL)
        .with_retry(Duration::from_millis(50), 5)
        .with_retries(0)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 503);
    assert_eq!(fetch_calls(), 1);

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_failed_fetch_is_replayed_after_the_pause() {
    script_fetch(vec![NETWORK_FAILURE, 200]);

    let started = js_sys::Date::now();

    let response = FlUrl::new(URL)
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await
        .unwrap();

    let elapsed = js_sys::Date::now() - started;

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(fetch_calls(), 2);
    assert!(
        elapsed >= millis(RETRY_DELAY),
        "a pause of {:?} took only {} ms",
        RETRY_DELAY,
        elapsed
    );

    restore_fetch();
}

#[wasm_bindgen_test]
async fn a_timed_out_attempt_is_replayed() {
    script_fetch(vec![HANG, 200]);

    let response = FlUrl::new(URL)
        .set_timeout(Duration::from_millis(100))
        .with_retry(Duration::from_millis(50), 5)
        .get()
        .await
        .unwrap();

    assert_eq!(response.get_status_code(), 200);
    assert_eq!(fetch_calls(), 2);

    restore_fetch();
}

#[wasm_bindgen_test]
async fn when_the_attempts_run_out_a_failed_fetch_is_the_error() {
    script_fetch(vec![NETWORK_FAILURE, NETWORK_FAILURE, NETWORK_FAILURE]);

    let result = FlUrl::new(URL)
        .with_retry(Duration::from_millis(50), 2)
        .get()
        .await;

    assert!(
        matches!(result, Err(FlUrlError::FetchError(_))),
        "{:?}",
        result
    );
    assert_eq!(fetch_calls(), 3);

    restore_fetch();
}

/// Nothing went out, and a replay would be refused the same way — so there is no
/// outage to wait for.
#[wasm_bindgen_test]
async fn a_request_the_browser_refuses_to_build_is_not_replayed() {
    script_fetch(vec![]);

    let started = js_sys::Date::now();

    // A line break is never allowed in a header value.
    let result = FlUrl::new(URL)
        .with_header("X-Broken", "a\nb")
        .with_retry(RETRY_DELAY, 5)
        .get()
        .await;

    let elapsed = js_sys::Date::now() - started;

    assert!(result.is_err());
    assert_eq!(fetch_calls(), 0, "the request never reached fetch");
    assert!(
        elapsed < millis(RETRY_DELAY),
        "a request that can not be built was waited on for {} ms",
        elapsed
    );

    restore_fetch();
}

/// A url that can not be used leaves the builder with an error instead of a request:
/// every step after it is skipped, the request returns that error, and `fetch` is
/// never called — so there is nothing to replay either.
#[wasm_bindgen_test]
async fn a_url_that_can_not_be_used_is_the_error_of_the_request() {
    script_fetch(vec![]);

    let started = js_sys::Date::now();

    // Absolute, so no origin gets involved — and it names no host.
    let fl_url = FlUrl::new("https://")
        .append_path_segment("api")
        .with_header("X-Api-Key", "secret")
        .with_retry(RETRY_DELAY, 5);

    assert!(
        matches!(fl_url.get_error(), Some(FlUrlError::InvalidUrl(_))),
        "{:?}",
        fl_url.get_error()
    );
    assert!(matches!(
        fl_url.get_url_builder(),
        Err(FlUrlError::InvalidUrl(_))
    ));

    let result = fl_url.post(HttpRequestBody::as_json(&["payload"])).await;

    let elapsed = js_sys::Date::now() - started;

    match result {
        Err(FlUrlError::InvalidUrl(message)) => {
            assert_eq!(message, "Invalid url 'https://': it names no host")
        }
        other => panic!("{:?}", other),
    }
    assert_eq!(fetch_calls(), 0, "the request never reached fetch");
    assert!(
        elapsed < millis(RETRY_DELAY),
        "a request with no url to go to was waited on for {} ms",
        elapsed
    );

    // A url that can be used carries no error, and is there to be read.
    let fl_url = FlUrl::new(URL).append_path_segment("users");
    assert!(fl_url.get_error().is_none());
    assert_eq!(
        fl_url.get_url_builder().unwrap().get_path_and_query(),
        "/api/data/users"
    );

    restore_fetch();
}

/// The body is taken out of the response, once; the response keeps its head.
#[wasm_bindgen_test]
async fn the_body_is_taken_once() {
    script_fetch(vec![200]);

    let mut response = FlUrl::new(URL).get().await.unwrap();

    let body = response.get_body().unwrap();
    assert_eq!(response.get_status_code(), 200);
    assert!(response.get_body().is_err());

    assert_eq!(body.into_json::<u16>(usize::MAX).await.unwrap(), 200);
    assert_eq!(fetch_calls(), 1);

    restore_fetch();
}
