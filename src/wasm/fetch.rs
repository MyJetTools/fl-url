//! The browser `fetch` glue: build a `Request`, run it (with an optional
//! `AbortController`-based timeout), and read back status / headers / body.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Headers, Request, RequestInit, Response};

use crate::FlUrlError;

/// Performs one `fetch` round-trip and returns the raw `Response` (headers only —
/// the body is read lazily by [`read_response_body`]) together with the
/// `AbortController` bound to the request, so the caller can later bound the body
/// read on the same signal.
///
/// `request_timeout_millis` bounds only the request→headers round-trip (mirroring
/// the native `request_timeout`, which bounds `do_request`). The timer is cleared
/// the instant the response resolves, so it never fires against the body read.
pub(crate) async fn execute_fetch(
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    request_timeout_millis: Option<i32>,
    print_input_request: bool,
) -> Result<(Response, Option<AbortController>), FlUrlError> {
    let init = RequestInit::new();
    init.set_method(method);

    let web_headers = Headers::new().map_err(js_to_err)?;
    for (name, value) in headers {
        web_headers.append(name, value).map_err(js_to_err)?;
    }
    init.set_headers_headers(&web_headers);

    if let Some(body) = body {
        if !body.is_empty() {
            let array = js_sys::Uint8Array::from(body);
            init.set_body(array.as_ref());
        }
    }

    // A controller is always attached so the (later) body read can be bounded on
    // the same signal; it is only ever aborted if a timeout timer fires.
    let controller = AbortController::new().ok();
    if let Some(controller) = controller.as_ref() {
        init.set_signal(Some(&controller.signal()));
    }

    if print_input_request {
        web_sys::console::log_1(&JsValue::from_str(&format!("[{}] {}", method, url)));
    }

    let request = Request::new_with_str_and_init(url, &init).map_err(js_to_err)?;

    let timed_out = Rc::new(Cell::new(false));
    let timer_handle = match (controller.as_ref(), request_timeout_millis) {
        (Some(controller), Some(millis)) => set_abort_timer(controller, millis, timed_out.clone()),
        _ => None,
    };

    let promise = fetch_promise(&request)?;
    let result = JsFuture::from(promise).await;

    // The continuation after `.await` is a microtask and runs before any pending
    // `setTimeout` macrotask, so clearing here guarantees the request timer can
    // never fire against the still-attached response body.
    if let Some(handle) = timer_handle {
        clear_timer(handle);
    }

    match result {
        Ok(value) => {
            let response = value.dyn_into::<Response>().map_err(|_| {
                FlUrlError::FetchError("fetch did not return a Response".to_string())
            })?;
            Ok((response, controller))
        }
        Err(err) => {
            // We only ever abort on timeout, so an AbortError here means our timer
            // fired.
            if timed_out.get() || is_abort_error(&err) {
                Err(FlUrlError::Timeout)
            } else {
                Err(js_to_err(err))
            }
        }
    }
}

/// Buffers the whole response body into memory (`Response.arrayBuffer()`).
///
/// When `body_timeout_millis` is set (from `set_response_body_timeout`), the read
/// is bounded on the request's `AbortController`; the resulting abort is surfaced
/// as [`FlUrlError::Timeout`]. With no body timeout the read is unbounded,
/// matching the native default (`response_body_timeout = None`).
pub(crate) async fn read_response_body(
    response: &Response,
    controller: Option<&AbortController>,
    body_timeout_millis: Option<i32>,
) -> Result<Vec<u8>, FlUrlError> {
    let timed_out = Rc::new(Cell::new(false));
    let timer_handle = match (controller, body_timeout_millis) {
        (Some(controller), Some(millis)) => set_abort_timer(controller, millis, timed_out.clone()),
        _ => None,
    };

    let promise = match response.array_buffer() {
        Ok(promise) => promise,
        Err(err) => {
            if let Some(handle) = timer_handle {
                clear_timer(handle);
            }
            return Err(js_to_err(err));
        }
    };

    let result = JsFuture::from(promise).await;

    if let Some(handle) = timer_handle {
        clear_timer(handle);
    }

    match result {
        Ok(value) => {
            let array_buffer = value.dyn_into::<js_sys::ArrayBuffer>().map_err(|_| {
                FlUrlError::FetchError(
                    "Response.arrayBuffer() did not return an ArrayBuffer".to_string(),
                )
            })?;
            Ok(js_sys::Uint8Array::new(&array_buffer).to_vec())
        }
        Err(err) => {
            if timed_out.get() || is_abort_error(&err) {
                Err(FlUrlError::Timeout)
            } else {
                Err(js_to_err(err))
            }
        }
    }
}

/// Reads all response headers into a `(name, value)` list. The browser lower-cases
/// header names, so callers should look them up case-insensitively.
pub(crate) fn collect_headers(headers: &Headers) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let entries = headers.entries();
    loop {
        let next = match entries.next() {
            Ok(next) => next,
            Err(_) => break,
        };
        if next.done() {
            break;
        }
        if let Ok(pair) = next.value().dyn_into::<js_sys::Array>() {
            let name = pair.get(0).as_string().unwrap_or_default();
            let value = pair.get(1).as_string().unwrap_or_default();
            result.push((name, value));
        }
    }
    result
}

/// Returns the current origin (e.g. `https://example.com`) of the page or worker,
/// with any trailing slash stripped. Used to resolve a request URL that is not an
/// absolute `http(s)` URL (e.g. `/api/xxx`) against the current origin, the same
/// way the browser resolves a relative `fetch`.
pub(crate) fn get_origin() -> Result<String, FlUrlError> {
    let global = js_sys::global();

    let origin = if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        window.location().origin().map_err(js_to_err)?
    } else if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        scope.location().origin()
    } else {
        return Err(FlUrlError::FetchError(
            "global scope is neither Window nor WorkerGlobalScope; cannot resolve origin for a relative URL"
                .to_string(),
        ));
    };

    Ok(origin.strip_suffix('/').unwrap_or(&origin).to_string())
}

/// `fetch` lives on the global scope, which is a `Window` in a page and a
/// `WorkerGlobalScope` in a worker — support both.
fn fetch_promise(request: &Request) -> Result<js_sys::Promise, FlUrlError> {
    let global = js_sys::global();

    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        return Ok(window.fetch_with_request(request));
    }

    if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        return Ok(scope.fetch_with_request(request));
    }

    Err(FlUrlError::FetchError(
        "global scope is neither Window nor WorkerGlobalScope; fetch is unavailable".to_string(),
    ))
}

/// Arms `controller.abort()` after `millis` via `setTimeout` and returns the timer
/// handle (so the caller can `clearTimeout` it once the awaited op settles).
fn set_abort_timer(controller: &AbortController, millis: i32, timed_out: Rc<Cell<bool>>) -> Option<i32> {
    let controller = controller.clone();
    let closure = Closure::once_into_js(move || {
        timed_out.set(true);
        controller.abort();
    });
    let handler = closure.unchecked_ref::<js_sys::Function>();

    let global = js_sys::global();
    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        return window
            .set_timeout_with_callback_and_timeout_and_arguments_0(handler, millis)
            .ok();
    }
    if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        return scope
            .set_timeout_with_callback_and_timeout_and_arguments_0(handler, millis)
            .ok();
    }
    None
}

fn clear_timer(handle: i32) {
    let global = js_sys::global();
    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        window.clear_timeout_with_handle(handle);
    } else if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        scope.clear_timeout_with_handle(handle);
    }
}

fn is_abort_error(err: &JsValue) -> bool {
    err.dyn_ref::<web_sys::DomException>()
        .map(|exception| exception.name() == "AbortError")
        .unwrap_or(false)
}

pub(crate) fn js_to_err(value: JsValue) -> FlUrlError {
    FlUrlError::FetchError(describe_js_value(&value))
}

fn describe_js_value(value: &JsValue) -> String {
    if let Some(exception) = value.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", exception.name(), exception.message());
    }
    if let Some(text) = value.as_string() {
        return text;
    }
    format!("{:?}", value)
}
