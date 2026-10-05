# WebAssembly (WASM) Support

`flurl` is a **single crate that compiles for both native and `wasm32`**, and the
backend is chosen automatically by target — the public API (`FlUrl`,
`FlUrlResponse`, `FlUrlError`, `FlUrlHeaders`, `IntoFlUrl`, `body::*`, …) is the
same on both, so the same call sites compile everywhere:

```rust
use flurl::FlUrl;

// Identical code on native and in the browser:
let mut response = FlUrl::new("https://api.example.com")
    .append_path_segment("users")
    .with_header("Authorization", "Bearer token")
    .get()
    .await?;

let users: Vec<User> = response.get_json().await?;
```

## How it is wired

| Target | Backend (`cfg`) | Transport |
| --- | --- | --- |
| non-wasm | `src/non_wasm/` — full hyper/tokio impl | HTTP/1.1 & HTTP/2, TLS, client certs, connection pooling, unix sockets, SSH |
| `wasm32-unknown-unknown` | `src/wasm/` | the browser `fetch` API via `web-sys` |

Neither backend module is public: both alias their types to the crate root — use
`flurl::FlUrl`, never a backend path — and the shared pieces
(`FlUrlError` and the request `body` types) live at the root and are used by both. Native-only dependencies (hyper, tokio, my-tls, …) are
excluded from the wasm build; the wasm build pulls only `web-sys` / `wasm-bindgen`.

Add it to a wasm project exactly like a normal dependency (no extra feature
needed — the target is detected automatically):

```toml
[dependencies]
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git" }
```

## What differs under wasm

Because the browser owns the connection pool, TLS and redirects, the following
native knobs are kept for signature parity but are **no-ops** under wasm:
`set_connections_cache`, `accept_invalid_certificate`, `do_not_reuse_connection`,
`update_mode`, `accept_gzip` (the browser decompresses transparently),
`set_not_used_connection_timeout`.

Redirects are followed by the browser, so under wasm you get the response at the end
of them; natively FlUrl returns the redirect as it is — see [Redirects](index_resource.md#redirects).

These **do** work under wasm: `set_timeout` bounds the request→headers round-trip
via `AbortController` + `setTimeout`; `set_response_body_timeout` bounds the body
read on the same signal (unbounded by default, as on native); `with_retries` and
`with_retry` replay idempotent methods only, with the same semantics as on native
(the `with_retry` pause is a `setTimeout`-driven future); `compress` gzips the
request body.

Native-only surface that is **not available** under wasm (browsers can't express
it): `with_client_certificate` (native + a TLS provider feature), all `*_ssh_*` methods, unix-socket URLs,
`get_body_as_stream` / `FlResponseAsStream`, `into_hyper_response`, the
`*_request_streamed` / `execute_streamed` methods and `get_resolved_ip`.

Futures returned under wasm are `!Send` (the browser is single-threaded), so drive
them with `wasm_bindgen_futures::spawn_local` / your framework's async context
rather than `tokio::spawn`. The `.await` call sites are unchanged.

## Relative URLs (origin resolution, wasm-only)

Under wasm you can pass a **root-relative** URL — anything that does not start with
`http://` or `https://`, e.g. `/api/users` — and FlUrl resolves it against the
current page (or web-worker) origin before the request, exactly the way the browser
resolves a relative `fetch`:

```rust
// In a page served from https://my-app.com:
let response = FlUrl::new("/api/dashboards/v1/ab-books-compare")
    .get()
    .await?;
// → GET https://my-app.com/api/dashboards/v1/ab-books-compare
```

The origin is read via `web-sys` from `Window.location` (or, in a worker,
`WorkerGlobalScope.location`), so no base URL has to be threaded through your code.
Absolute `http(s)://…` URLs are used as-is. The prefix is applied **before** the URL
is parsed, so the parser always sees a well-formed absolute URL.

This resolution is wasm-only: on native there is no ambient origin, so pass an
absolute URL there.
