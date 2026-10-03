# FLUrl

A fluent, async HTTP client library for Rust, inspired by the .NET Flurl library (https://flurl.dev/).

FLUrl is a Hyper-based HTTP client that provides a fluent API for building and executing HTTP requests with connection pooling, retry logic, and comprehensive body type support.

## Features

- **Fluent API**: Chain methods to build requests naturally
- **Connection Reuse**: Automatic connection pooling and reuse for HTTP/1.1 and HTTP/2
- **Multiple HTTP Modes**: Support for HTTP/2, HTTP/1.1 with Hyper, and HTTP/1.1 without Hyper
- **Body Types**: JSON, URL-encoded, multipart/form-data, and raw data
- **SSL/TLS**: Opt-in via one of two provider features — `with-ring-tls` (ring) or `with-rust-tls` (pure Rust, no C toolchain). Client certificate support and invalid certificate acceptance. With neither, the crate never links rustls and an `https://` request fails with `FlUrlError::UnsupportedScheme`
- **SSH Tunneling**: Optional SSH tunnel support via `with-ssh` feature
- **Unix Socket Support**: Native Unix socket support (Unix systems only)
- **Known IP, no DNS**: `https://domain.com@15.0.0.5/path` connects to the ip while the Host header and TLS SNI stay `domain.com` (native only) — see [Connecting to a Known IP](#connecting-to-a-known-ip-no-dns-native-only)
- **Retry Logic**: `with_retries(n)` replays a request that got no response; `with_retry(delay, n)` also replays a 5xx, `delay` apart, to ride out a restart of the service — see [Retry Logic](#retry-logic)
- **Request Compression**: `compress()` gzips a request body of 64 bytes and up
- **Streaming Responses**: Support for streaming response bodies (native only)
- **Streaming Request Bodies**: Send a body of any size at constant memory, framed with `Content-Length` or chunked (native only) — see [Streamed Body](#streamed-body-native-only)
- **Debug Support**: Built-in request debugging capabilities
- **WASM Support**: The same API compiles to `wasm32-unknown-unknown` (browser / web-worker) on top of the `fetch` API — see [WebAssembly (WASM) Support](#webassembly-wasm-support)

## Installation

`flurl` is not published on crates.io — it is pulled from git, pinned to a release
tag. Add to your `Cargo.toml`:

```toml
[dependencies]
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git" }
```

**`https://` needs a TLS provider feature.** Both are off by default, so a
project doing plain HTTP (or unix sockets, or SSH tunnels) does not pay for the
rustls stack — `my-tls`, `rustls`, `tokio-rustls` and the provider itself all
leave the dependency tree. In a build with neither, a request to an `https://` url
returns `FlUrlError::UnsupportedScheme` — `FlUrl does not support https: it is
compiled without a TLS provider feature` — before a socket is opened.

Pick one:

```toml
[dependencies]
# ring — the default recommendation: mature, very widely deployed.
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git", features = [
    "with-ring-tls",
] }
```

```toml
[dependencies]
# pure Rust — no C toolchain anywhere, via rustls-graviola.
# x86_64 and aarch64 only; a younger, less deployed crypto implementation.
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git", features = [
    "with-rust-tls",
] }
```

Enabling both is not an error (`--all-features` does it): my-tls resolves the
conflict in favour of ring.

For SSH tunneling support:

```toml
[dependencies]
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git", features = [
    "with-ssh",
] }
```

### Feature flags

| Feature | Default | What it does |
| --- | --- | --- |
| `with-ring-tls` | off | TLS on the **ring** provider. Enables `https://` plus [`with_client_certificate`](#client-certificate). Mature and widely deployed; costs a bundled C/assembly build. No `aws-lc-sys` either way. |
| `with-rust-tls` | off | The same, on a **pure-Rust** provider (`rustls-graviola`) — no C toolchain at all. Builds only on x86_64 and aarch64, and the implementation is far younger than ring. Prefer `with-ring-tls` unless dropping the C toolchain is the point. |
| `dangerous-tls` | off | A modifier, not a TLS switch: it makes [`accept_invalid_certificate()`](#accept-invalid-certificates) actually skip server-cert verification. Combine it with a provider feature — on its own the TLS code compiles but no provider is installed, so https fails at connect time. |
| `with-ssh` | off | [SSH tunneling](#ssh-tunneling-with-ssh-feature) (`ssh://…->http://…` urls). Unix only. Built on `my-ssh`, which is `russh` underneath — no `libssh2` or OpenSSL, but this is the one feature that links `aws-lc-sys` (a bundled C build). |

On `wasm32` TLS is the browser's job, so neither provider feature matters there —
the `fetch` backend handles `https://` with or without them.

## WebAssembly (WASM) Support

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

### How it is wired

| Target | Backend (`cfg`) | Transport |
| --- | --- | --- |
| non-wasm | `src/non_wasm/` — full hyper/tokio impl | HTTP/1.1 & HTTP/2, TLS, client certs, connection pooling, unix sockets, SSH |
| `wasm32-unknown-unknown` | `src/wasm/` | the browser `fetch` API via `web-sys` |

Neither backend module is public: both alias their types to the crate root — use
`flurl::FlUrl`, never a backend path — and the shared pieces
(`FlUrlError`, the request `body` types, the drop-connection scenario) live at the
root and are used by both. Native-only dependencies (hyper, tokio, my-tls, …) are
excluded from the wasm build; the wasm build pulls only `web-sys` / `wasm-bindgen`.

Add it to a wasm project exactly like a normal dependency (no extra feature
needed — the target is detected automatically):

```toml
[dependencies]
flurl = { tag = "0.7.0", git = "https://github.com/MyJetTools/fl-url.git" }
```

### What differs under wasm

Because the browser owns the connection pool, TLS and redirects, the following
native knobs are kept for signature parity but are **no-ops** under wasm:
`set_connections_cache`, `accept_invalid_certificate`, `do_not_reuse_connection`,
`update_mode`, `accept_gzip` (the browser decompresses transparently),
`set_not_used_connection_timeout`.

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

### Relative URLs (origin resolution, wasm-only)

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

## Basic Usage

### Simple GET Request

```rust
use flurl::FlUrl;

let response = FlUrl::new("http://mywebsite.com")
    .append_path_segment("api")
    .append_path_segment("users")
    .append_query_param("page", Some("1"))
    .append_query_param("limit", Some("10"))
    .get()
    .await?;
```

### Errors Come From the Request

`FlUrl` is a builder: `FlUrl::new` and every method chained after it return the
builder, never a `Result`. What one of them can not use — the url, a header, a client
certificate — is remembered instead: from the first such error on every step is
skipped, and the call that sends the request returns that error without sending
anything. So one `?`, or one `match`, at the end covers the whole chain:

```rust
use flurl::{FlUrl, FlUrlError};

match FlUrl::new(url_from_settings)
    .append_path_segment("api")
    .with_header("X-Api-Key", api_key)
    .get()
    .await
{
    Ok(response) => {
        // A response arrived
    }
    Err(FlUrlError::InvalidUrl(reason)) => {
        // The url from settings can not be used
        eprintln!("Invalid URL: {}", reason);
    }
    Err(err) => {
        eprintln!("Error: {}", err);
    }
}
```

`FlUrl::new` does not panic on a url it can not use.

To check a url without sending anything — at start-up, say — ask the builder for the
error it is carrying. `get_url_builder()` gives the url as it stands, and is `None`
once there is an error:

```rust
use flurl::FlUrl;

if let Some(err) = FlUrl::new(url_from_settings).get_error() {
    eprintln!("Invalid url in settings: {}", err);
}
```

The request fails with `FlUrlError::InvalidUrl` for a url that can not be used at all:

| url | why |
| --- | --- |
| `""`, `" "`, `http://`, `http://:8080`, `http:///path`, `http+unix://` | it names no host (for a unix socket — no socket file) |
| a host, or a socket path, that does not fit into 255 bytes together with its port | it is too long to be a host — usually something else pasted into the url's place |
| `ftp://host` | the scheme is not one FlUrl knows |
| `https://domain.com@backend.internal` | a malformed [`name@ip`](#connecting-to-a-known-ip-no-dns-native-only) form |
| `http://user@host:22->…`, `ssh://user@host:22x->…` | a malformed [ssh tunnel](#ssh-tunneling-with-ssh-feature) part |

A url with no scheme is fine — `localhost:8080`, `10.0.0.1:5123/api` — and is taken
as `http://`. That is the native backend; under wasm a url that does not start with
`http://` or `https://` is resolved against the page origin instead — see
[Relative URLs](#relative-urls-origin-resolution-wasm-only).

A url that parses but can not be put on the wire fails the request as well: a new
line or another control byte in the host, a space in the path (a value read from a
file together with its trailing new line is the usual source).

### Using String Literals (IntoFlUrl Trait)

```rust
use flurl::IntoFlUrl;

let response = "http://mywebsite.com"
    .append_path_segment("Row")
    .append_query_param("tableName", Some(table_name))
    .append_query_param("partitionKey", Some(partition_key))
    .get()
    .await?;
```

## HTTP Methods

Each of them has a `*_with_debug` twin that also dumps the request into a `&mut String`
— see [Request Debug String](#request-debug-string) for the full list.

### GET

```rust
let response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;
```

### GET with Debug

```rust
let mut debug_string = String::new();
let response = FlUrl::new("https://api.example.com/data")
    .get_with_debug(&mut debug_string)
    .await?;
println!("Request: {}", debug_string);
```

### POST

```rust
use flurl::body::HttpRequestBody;

let body = HttpRequestBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/users")
    .post(body)
    .await?;
```

### POST with Debug

```rust
let mut debug_string = String::new();
let body = HttpRequestBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/users")
    .post_with_debug(body, &mut debug_string)
    .await?;
```

### PUT

```rust
let body = HttpRequestBody::as_json(&update_data);
let response = FlUrl::new("https://api.example.com/users/123")
    .put(body)
    .await?;
```

### PATCH

```rust
let body = HttpRequestBody::as_json(&patch_data);
let response = FlUrl::new("https://api.example.com/users/123")
    .patch(body)
    .await?;
```

### DELETE

```rust
let response = FlUrl::new("https://api.example.com/users/123")
    .delete()
    .await?;
```

### DELETE with Debug

```rust
let mut debug_string = String::new();
let response = FlUrl::new("https://api.example.com/users/123")
    .delete_with_debug(&mut debug_string)
    .await?;
```

### HEAD

```rust
let response = FlUrl::new("https://api.example.com/resource")
    .head()
    .await?;
```

## URL Building

### Append Path Segments

```rust
let response = FlUrl::new("https://api.example.com")
    .append_path_segment("api")
    .append_path_segment("v1")
    .append_path_segment("users")
    .get()
    .await?;
// Results in: https://api.example.com/api/v1/users
```

### Append Query Parameters

```rust
let response = FlUrl::new("https://api.example.com/search")
    .append_query_param("q", Some("rust"))
    .append_query_param("page", Some("1"))
    .append_query_param("sort", None::<&str>) // Adds parameter without value
    .get()
    .await?;
// Results in: https://api.example.com/search?q=rust&page=1&sort
```

### Append Raw URL Ending

```rust
let response = FlUrl::new("https://api.example.com")
    .append_raw_ending_to_url("/custom/path?param=value")
    .get()
    .await?;
```

## Headers

### Add Custom Headers

```rust
let response = FlUrl::new("https://api.example.com/data")
    .with_header("Authorization", "Bearer token123")
    .with_header("X-Custom-Header", "value")
    .get()
    .await?;
```

A header that must not be put on the wire fails the request with
`FlUrlError::RequestBuild` — nothing is sent, and the header is not silently dropped:

- a value with a CR, LF or NUL in it — typically an api key read from a file together
  with its trailing new line;
- a name that is empty or is not an HTTP token (a space, a `:`, a non-ASCII letter).

`with_header` itself can not report this (it returns the builder), so the error comes
from the call that sends the request. The message names the header and the byte, never
the value. The same applies to the header fields of a
[request model](#model-driven-requests).

That check is the native backend's. Under wasm a header goes to the browser as it is,
and one the browser refuses fails the request with `FlUrlError::FetchError`.

## Request Bodies

### JSON Body

```rust
use flurl::body::HttpRequestBody;
use serde::Serialize;

#[derive(Serialize)]
struct User {
    name: String,
    email: String,
}

let user = User {
    name: "John Doe".to_string(),
    email: "john@example.com".to_string(),
};

let response = FlUrl::new("https://api.example.com/users")
    .post(HttpRequestBody::as_json(&user))
    .await?;
```

### URL-Encoded Body

```rust
use flurl::body::UrlEncodedBody;

let body = UrlEncodedBody::new()
    .append("username", "john")
    .append("password", "secret123")
    .append("remember", "true");

let response = FlUrl::new("https://api.example.com/login")
    .post(body)
    .await?;
```

### Multipart Form Data

```rust
use flurl::body::new_form_data;

// Form fields (`new_form_data()` generates a random multipart boundary)
let form_data = new_form_data()
    .append_form_data_field("username", "john")
    .append_form_data_field("email", "john@example.com");

let response = FlUrl::new("https://api.example.com/profile")
    .post(form_data)
    .await?;

// Form with file upload
let form_data = new_form_data()
    .append_form_data_field("title", "My Document")
    .append_form_data_file("file", "document.pdf", "application/pdf", &file_bytes);

let response = FlUrl::new("https://api.example.com/upload")
    .post(form_data)
    .await?;
```

### Raw Body

```rust
use flurl::body::HttpRequestBody;

let raw_data = b"custom binary data";
let body = HttpRequestBody::from_raw_data(raw_data.to_vec(), Some("application/octet-stream"));

let response = FlUrl::new("https://api.example.com/upload")
    .post(body)
    .await?;
```

### Streamed Body (native only)

Every body above is a `Vec<u8>`: the payload exists in memory as a whole, and a large
upload costs its own size in RSS — twice, if the caller also read a file to build it.
For a body that must not be materialized, `post_request_streamed` /
`put_request_streamed` / `patch_request_streamed` take anything implementing
`hyper::body::Body<Data = Bytes>` and write it to the socket as it is produced. Peak
memory is one chunk plus whatever the producer buffers, whatever the size of the body.

`my_http_client::RequestBodyStream` is the usual producer — a body over an mpsc
channel, where the channel is the backpressure: `publish` waits once `buffer` chunks
are queued for the socket. **Dropping the publisher is what ends the body.**

`my_http_client` is a crate of its own, and `flurl` does not re-export it. To name
`RequestBodyStream`, add it to `Cargo.toml` next to `flurl` — the same repository and
tag `flurl` itself is built on:

```toml
[dependencies]
my-http-client = { tag = "0.1.0", git = "https://github.com/my-jet-tools/my-http-client.git" }
```

```rust
use my_http_client::RequestBodyStream;

let (publisher, body) = RequestBodyStream::new(4);

tokio::spawn(async move {
    while let Some(chunk) = source.next().await {
        // Err means the request is over — nothing else can be published
        if publisher.publish(chunk).await.is_err() {
            break;
        }
    }
    // dropping the publisher ends the body
});

let response = FlUrl::new("https://api.example.com")
    .append_path_segment("upload")
    .set_timeout(Duration::from_secs(600))
    // None: the size is unknown, so the body goes out chunked
    .post_request_streamed(body, None)
    .await?;
```

A proxied `hyper::body::Incoming`, a `StreamBody` over a file reader, or any other
`Body` implementation works just as well — and needs no `my-http-client` in
`Cargo.toml`. `hyper` itself is re-exported as `flurl::hyper`.

#### Framing: the `content_length` argument

HTTP/1.1 offers exactly two ways to delimit a request body, and the last argument
picks between them:

| argument | framing | when |
| --- | --- | --- |
| `None` | `Transfer-Encoding: chunked` | the size is genuinely unknown. Every HTTP/1.1 recipient is required to understand chunked, so this is the right default |
| `Some(n)` | `Content-Length: n` | the size is known, or the endpoint refuses a chunked request body |

```rust
let response = FlUrl::new("https://api.example.com")
    .append_path_segment("files")
    .append_path_segment("archive.tar")
    .set_timeout(Duration::from_secs(600))
    .put_request_streamed(body, Some(len))
    .await?;
```

What lands on the wire is then plain length framing:

```
PUT /files/archive.tar HTTP/1.1
content-length: 524288
host: api.example.com
```

With `Some(n)` the body must then deliver **exactly** `n` bytes. That is not an fl-url
rule but the protocol's: a short body makes the message incomplete, and extra bytes
would be read as the start of the next request on the same connection. A stream that
ends early therefore fails the request with `"user body write aborted"` instead of
leaving a truncated payload behind — so `n` and the producer have to come from one
source (a file's metadata and that same file), never be computed twice.

The argument is the single source of the framing, so it overrides a `Content-Length`
added with `with_header` in **both** directions: `Some(n)` replaces such a header
(never emits a second one, which would be a protocol violation), and `None` removes
it — a body of unknown size must not claim a length it may not deliver.

#### What does not apply to a streamed body

| knob | what happens |
| --- | --- |
| `compress()` | `FlUrlError::StreamedBodyCanNotBeCompressed` — gzip needs the whole body in one buffer, which is exactly what streaming avoids |
| `with_retries(n)`, `with_retry(d, n)` | ignored: the payload is consumed as it is sent, so the request is attempted **exactly once**. Rebuilding the stream and calling again is the caller's decision — it owns the source data |
| `set_timeout(d)` | now covers the **whole** call, upload included, not just the wait for the response head. The 10s default is far too short for a real upload |
| `update_mode(..)` | ignored: the mode is pinned to `Http1Hyper`, since the own HTTP/1.1 implementation serializes a request into one buffer and the h2 client has no streaming entry point |

These methods are native-only — the browser `fetch` API cannot stream a request body
without HTTP/2 duplex, so there is no wasm counterpart.

## Model-Driven Requests

Instead of wiring up the path, query, headers, and body by hand, you can describe a
request with a `my_http_utils` model (any type deriving
`my_http_utils::macros::MyHttpInput`) and hand it to `execute_request`. The model
fills the URL path/query, headers, and body; the `HttpVerb` selects the method. The
base host and any static route prefix are still configured on the builder beforehand.

`my_http_utils` is re-exported as `flurl::my_http_utils`, so it does not have to be in
`Cargo.toml` — and taking it from there is what makes the model implement the very
trait `execute_request` accepts. The derive expands to paths that start with
`my_http_utils::`, so that name has to be in scope: `use flurl::my_http_utils;`.

```rust
use flurl::my_http_utils;
use flurl::{FlUrl, HttpVerb};
use my_http_utils::macros::MyHttpInput;

#[derive(MyHttpInput)]
struct CreateUser {
    #[http_path(name = "orgId", description = "")]
    org_id: String,
    #[http_query(name = "notify", description = "")]
    notify: bool,
    #[http_header(name = "X-Api-Key", description = "")]
    api_key: String,
    #[http_body(name = "name", description = "")]
    name: String,
}

let model = CreateUser {
    org_id: "org-42".to_string(),
    notify: true,
    api_key: "secret".to_string(),
    name: "John".to_string(),
};

// Base host + static route prefix set by the caller, the model fills the rest.
let response = FlUrl::new("https://api.example.com")
    .append_path_segment("api")
    .append_path_segment("users")
    .execute_request(HttpVerb::Post, model)
    .await?;
// POST https://api.example.com/api/users/org-42?notify=true
//   X-Api-Key: secret
//   { "name": "John" }
```

`Get`/`Delete`/`Head` do not carry a body, so a body produced by the model is
ignored for those verbs.

### A Model That Streams Its Body (native only)

A model whose body field is marked `#[http_body_as_stream]` is sent the streamed way
by the very same `execute_request` — the chunks the application writes into the
stream go to the socket as they arrive, and the payload is never materialized. It is
the same model the server parses the incoming body with, used from the other end.

```rust
use flurl::my_http_utils;
use flurl::{FlUrl, HttpVerb};
use my_http_utils::http_input::HttpBodyAsStream;
use my_http_utils::macros::MyHttpInput;

#[derive(MyHttpInput)]
struct UploadHttpInput {
    #[http_path(name = "fileName", description = "File name")]
    file_name: String,
    #[http_header(name = "X-Api-Key", description = "Api key")]
    api_key: String,
    #[http_body_as_stream(description = "File content")]
    body: HttpBodyAsStream,
}

// `4` is the channel capacity — the back-pressure knob; `None` = the size is not
// known up front, so the body goes out chunked.
let (sender, stream) = HttpBodyAsStream::create(4, None);

tokio::spawn(async move {
    while let Some(chunk) = source.next().await {
        // false means the transport is gone — nothing left to write into
        if !sender.send_chunk(chunk).await {
            return;
        }
    }
    // Marks the body complete. WITHOUT it a dropped sender reads as a producer that
    // died half-way, and the request fails instead of sending a truncated payload.
    sender.finish();
});

let response = FlUrl::new("https://api.example.com")
    .append_path_segment("upload")
    .with_header("Content-Type", "application/octet-stream")
    .set_timeout(Duration::from_secs(600))
    .execute_request(HttpVerb::Post, UploadHttpInput {
        file_name: "archive.tar".to_string(),
        api_key: "secret".to_string(),
        body: stream,
    })
    .await?;
```

The framing is not a separate argument here — it comes from the stream, so it can
not drift out of step with the payload: the `content_length` given to
`HttpBodyAsStream::create` becomes `Content-Length: n`, and `None` goes out chunked.
Everything else listed under [What does not apply to a streamed
body](#what-does-not-apply-to-a-streamed-body) applies unchanged — no `compress()`,
no retries, and `set_timeout` covers the whole upload.

Two cases fail the request rather than quietly sending something else:

| case | why |
| --- | --- |
| `Get` / `Delete` / `Head` | a materialized body is merely dropped for these verbs, but dropping a *stream* would leave the application writing into something nothing will ever read |
| `HttpBodyAsStream::empty()` | what a model carries when it is only ever parsed by a server; sending it would produce a request with no body at all |

**wasm**: the browser `fetch` API has no portable streamed request body — a
`ReadableStream` body needs Chromium 105+ over HTTP/2+, and Firefox and Safari do not
support it at all — so `execute_request` with such a model returns
`FlUrlError::RequestBuild` there instead of sending an empty body. Where the payload
is a file the user picked, streaming it by hand is not needed anyway: handing the
`File`/`Blob` straight to `fetch` makes the browser stream it from disk itself, at
constant memory, in every browser.

### Requests Without an Input Model

When a request carries no input model, pass `EmptyRequestModel` instead of deriving
a dedicated one. The URL and headers already set on the builder are used as-is, and
body-carrying verbs (`Post`/`Put`/`Patch`) send an empty body:

```rust
use flurl::{FlUrl, EmptyRequestModel, HttpVerb};

let response = FlUrl::new("https://api.example.com")
    .append_path_segment("health")
    .execute_request(HttpVerb::Get, EmptyRequestModel)
    .await?;
```

`EmptyRequestModel` is a shared, transport-agnostic stub that implements
`THttpRequestBuilder` as a no-op — it appends nothing to the URL, adds no headers,
and produces an empty body. It is the parameter-less stand-in for `execute_request`
on both the native and wasm backends, so you don't have to spell out a model type
(the way a bare `None` would have forced you to) just to satisfy the signature.

## Response Handling

### Get Status Code

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let status_code = response.get_status_code();
println!("Status: {}", status_code);
```

### Get Body as Slice

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body = response.get_body_as_slice().await?;
println!("Body length: {}", body.len());
```

### Get Body as String

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body = response.get_body_as_str().await?;
println!("Body: {}", body);
```

### Get JSON Response

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiResponse {
    data: Vec<String>,
}

let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let api_response: ApiResponse = response.get_json().await?;
```

### Receive Full Body

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body_bytes = response.receive_body().await?;
```

### Streaming Response

```rust
let response = FlUrl::new("https://api.example.com/large-file")
    .get()
    .await?;

let mut stream = response.get_body_as_stream();
while let Some(chunk) = stream.get_next_chunk().await? {
    // Process chunk
    println!("Received {} bytes", chunk.len());
}
```

The chunks are read off the connection as they come. Called after a buffered read
(`get_body_as_slice`, `get_json`, …), `get_body_as_stream` gives the body that read
loaded; after one that failed, its first chunk fails with the reason.

### Get Headers

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

// Get specific header
let content_type = response.get_header("Content-Type")?;

// Get header case-insensitive
let content_type = response.get_header_case_insensitive("content-type")?;

// Get all headers. The names come back lower-cased: `content-type`
let headers = response.get_headers();
for (key, value) in headers {
    println!("{}: {:?}", key, value);
}
```

## Connection Management

### Connection Reuse

By default, FLUrl reuses connections to avoid the cost of establishing new connections and TLS handshakes. A connection is reused by a request to the same scheme, host and port, made in the same [HTTP mode](#http-modes).

```rust
let mut response1 = FlUrl::new("https://api.example.com/endpoint1")
    .get()
    .await?;

// The connection goes back to the pool once the body has been read to the end
let body = response1.get_body_as_slice().await?;

let response2 = FlUrl::new("https://api.example.com/endpoint2")
    .get()
    .await?; // Reuses the connection response1 came on
```

### Disable Connection Reuse

```rust
let response = FlUrl::new("https://api.example.com/data")
    .do_not_reuse_connection()
    .get()
    .await?;
```

### Custom Connection Cache

```rust
use std::sync::Arc;
use flurl::FlUrlHttpConnectionsCache;

let cache = Arc::new(FlUrlHttpConnectionsCache::new());
let response = FlUrl::new("https://api.example.com/data")
    .set_connections_cache(cache.clone())
    .get()
    .await?;
```

### When a Connection Is Dropped

An HTTP/1.1 connection serves one request at a time. It is taken out of the pool for
the request and goes back only once the response body has been read to the end — by
`get_body_as_slice`, `get_json`, `get_body_as_str`, `receive_body`, or by a stream
read to its last chunk. Instead of going back it is closed, and the next request opens
a new one, when:

- the response has a status above 400 other than 404 — `401`, `403`, `500`, `503` and
  so on, but not `400` and not `404`;
- the response carries `Connection: close`;
- the response is dropped with its body unread, or reading the body fails or times out;
- the request itself fails;
- the request was made with `do_not_reuse_connection()`;
- the pool already holds its maximum of idle connections to that endpoint — 5 by
  default, `FlUrlHttpConnectionsCache::new_with_max_connections(n)` for another limit.

An HTTP/2 connection is shared instead: it stays in the pool while requests are
multiplexed over it, and leaves it when a request on it fails with anything other than
a request timeout.

Either kind is dropped once it has sat in the pool unused for longer than 120 seconds
— see [Connection Timeout](#connection-timeout) to change that.

These rules are fixed: there is no way to plug in a rule of your own. The crate
exports a `DropConnectionScenario` trait and its `DefaultDropConnectionScenario`, but
nothing takes an implementation of the trait.

## HTTP Modes

The default is `FlUrlMode::Http1Hyper`.

### HTTP/2

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::H2)
    .get()
    .await?;
```

### HTTP/1.1 with Hyper

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::Http1Hyper)
    .get()
    .await?;
```

### HTTP/1.1 without Hyper

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::Http1NoHyper)
    .get()
    .await?;
```

## SSL/TLS Configuration (needs a TLS provider feature)

Everything in this section requires `with-ring-tls` or `with-rust-tls`. Without one
`with_client_certificate` does not exist and an `https://` request fails with
`FlUrlError::UnsupportedScheme`.

### Accept Invalid Certificates

```rust
let response = FlUrl::new("https://self-signed.example.com")
    .accept_invalid_certificate()
    .get()
    .await?;
```

This one also needs `features = ["dangerous-tls"]` to take effect — with only
a provider feature the connection errors instead of silently dropping server-cert
verification.

### Client Certificate

```rust
use flurl::my_tls::ClientCertificate;

// A PKCS#12 file (.p12 / .pfx): the private key together with its certificate chain
let cert = ClientCertificate::load_pks12_from_file("client.p12", "password").await?;

let response = FlUrl::new("https://api.example.com/data")
    .with_client_certificate(cert)
    .get()
    .await?;
```

`ClientCertificate` is `my-tls`' type, re-exported as `flurl::my_tls`. It is built from
a PKCS#12 container only — there is no constructor that takes a pair of PEM files:

| constructor | a file that can not be read, a wrong password, a broken container |
| --- | --- |
| `ClientCertificate::load_pks12_from_file(file_name, password).await` | `Err(String)` |
| `ClientCertificate::from_pks12_file(file_name, password).await` | panics |
| `ClientCertificate::from_pkcs12(bytes, password)` | panics |

A key and a certificate kept as PEM files are packed into one with
`openssl pkcs12 -export -inkey client.key -in client.crt -out client.p12`.

`with_client_certificate` checks the certificate when it is given. What is wrong with
it becomes the error of the request — `FlUrlError::RequestBuild`, before a socket is
opened:

| case | message |
| --- | --- |
| the url is not `https://` — a client certificate is presented in the TLS handshake | `Client certificate can only be used with https. Url: …` |
| its private key can not be loaded — a broken or an unsupported key in the file | `Client certificate can not be used: …` |
| the request has been given a certificate already | `Client certificate is already set` |

A key that can not be loaded is found there, not in the middle of a connection attempt,
so it is not retried as an outage the way a failed connection is.

## SSH Tunneling (with-ssh feature)

### Basic SSH Tunnel

```rust
// Format: ssh://user@host:port->http://target-host:port
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .get()
    .await?;
```

The target behind the tunnel is spoken to in plain HTTP. There is no TLS layer over a
tunnel, so a `->https://…` target is not encrypted by it — keep the target `http://`.

With no credentials given, the session is opened with the keys of the running ssh
agent (`$SSH_AUTH_SOCK`). The methods below give it a password or a private key
instead. On a url that is not an ssh tunnel they do nothing, so the same code works
whether settings name `ssh://…->http://…` or a plain `http://…`.

### SSH with Password

```rust
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_password("password123")
    .get()
    .await?;
```

### SSH with Private Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, None) // None = no passphrase
    .get()
    .await?;
```

### SSH with Passphrase-Protected Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, Some("passphrase".to_string()))
    .get()
    .await?;
```

### SSH Credentials Resolver

A resolver supplies the credentials at the moment the request is sent, by the ssh
line of the tunnel — `user@host:port`:

```rust
use std::sync::Arc;
use flurl::my_ssh::ssh_settings::{SshPrivateKey, SshSecurityCredentialsResolver};

struct MySshResolver;

#[async_trait::async_trait]
impl SshSecurityCredentialsResolver for MySshResolver {
    // ssh_line is `user@host:port` of the tunnel
    async fn resolve_ssh_private_key(&self, ssh_line: &str) -> Option<SshPrivateKey> {
        if ssh_line != "user@ssh.example.com:22" {
            return None;
        }

        Some(SshPrivateKey {
            content: std::fs::read_to_string("id_rsa").ok()?,
            pass_phrase: None,
        })
    }

    async fn resolve_ssh_password(&self, ssh_line: &str) -> Option<String> {
        None
    }
}

let resolver = Arc::new(MySshResolver);
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_security_credentials_resolver(resolver)
    .get()
    .await?;
```

Both methods are required. The private key is asked for first and the password only
when there is none; when both return `None` the credentials stay what they were — the
ssh agent, or whatever `set_ssh_password` / `set_ssh_private_key` set. The trait has a
third method, `update_credentials`, with a default implementation that does exactly
this; override it to replace the credentials as a whole.

`my_ssh` is re-exported as `flurl::my_ssh`. The trait is an `async_trait` one, so the
`async-trait` crate has to be in `Cargo.toml`.

## Unix Socket Support (Unix systems only)

```rust
let response = FlUrl::new("http+unix:///var/run/docker.sock")
    .append_path_segment("containers")
    .append_path_segment("json")
    .get()
    .await?;
```

The socket file can be named in any of these ways; they all reach the same file:

| form | examples |
| --- | --- |
| the path itself | `/var/run/docker.sock`, `~/docker.sock` (`~` is resolved against `$HOME`) |
| a scheme — `http+unix`, `unix` or `unix+http`, in any case — and the path | `http+unix:///var/run/docker.sock`, `unix:///var/run/docker.sock`, `unix+http://var/run/docker.sock` |

After a scheme the path is absolute however many slashes are written: `unix:/var/run/x.sock`,
`unix://var/run/x.sock` and `unix:///var/run/x.sock` are all `/var/run/x.sock`.

The http path goes in with `append_path_segment`, as above. A url that names no socket
file — `http+unix://`, `unix://`, `/` — fails the request with
`FlUrlError::InvalidUrl`.

## Connecting to a Known IP (no DNS, native only)

When the address of the server is already known, put it after an `@` in the url:
`scheme://<server-name>@<ip>[:port]/path`. The socket is opened straight to the ip
— no DNS lookup — and everything the server sees still names `server-name`:

```rust
let response = FlUrl::new("https://domain.com@15.0.0.5/xxx/fff")
    .get()
    .await?;
// TCP connect → 15.0.0.5:443
// TLS SNI + certificate check → domain.com
// GET /xxx/fff
// Host: domain.com
```

| part | used for |
| --- | --- |
| `server-name` | the url's host: the `Host` header, h2 `:authority`, the TLS server name (SNI) and the name the certificate is verified against |
| `ip` | only where the TCP connection goes. IPv6 goes in brackets: `https://domain.com@[2001:db8::1]/` |
| `:port` | written after the ip; applies to both, so the Host header becomes `domain.com:8443` |

```rust
let response = FlUrl::new("https://domain.com@[2001:db8::1]:8443")
    .append_path_segment("api")
    .get()
    .await?;
// TCP connect → [2001:db8::1]:8443, Host: domain.com:8443
```

The part after `@` must be an ip — a host name there would need exactly the DNS
lookup this form exists to skip — and the part before it a bare server name (no
port, no `user:password`). Anything else fails the request with
`FlUrlError::InvalidUrl`. An `@` in the path or the query is not affected.

Pooled connections are keyed by the ip as well, so `domain.com@10.0.0.1` and
`domain.com@10.0.0.2` never share a connection. `fl_url.get_resolved_ip()` returns
the ip a url pinned.

The same form works behind an SSH tunnel (`ssh://user@host->http://domain.com@10.0.0.7:8080/`):
the ssh server is asked to connect to the ip instead of resolving the name.

Under wasm the browser owns DNS and TLS, so the form is not available there — the
browser refuses a url with an `@` in it.

## Advanced Configuration

### Timeouts

```rust
use std::time::Duration;

let mut response = FlUrl::new("https://api.example.com/data")
    .set_timeout(Duration::from_secs(30))
    .set_response_body_timeout(Duration::from_secs(60))
    .get()
    .await?;

let body = response.get_body_as_slice().await?;
```

| method | bounds | default | when it runs out |
| --- | --- | --- | --- |
| `set_timeout` | the request up to the response head — for a [streamed body](#what-does-not-apply-to-a-streamed-body) the upload as well | 10 seconds | the request fails with `FlUrlError::Timeout` |
| `set_response_body_timeout` | reading the response body: a buffered read as a whole, a stream chunk by chunk | unbounded | the read fails with `FlUrlError::Timeout` |

### Connection Timeout

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .set_not_used_connection_timeout(Duration::from_secs(60))
    .get()
    .await?;
```

How long a pooled connection may sit unused and still be handed out — 120 seconds by
default. The value is taken in whole seconds, rounded up, and is at least one. It
belongs to the request it is set on: taking a connection for that request drops the
ones to the same endpoint that have been idle for longer.

### Retry Logic

Two builder methods decide what happens when an attempt fails. They set one and the
same policy, so on a given `FlUrl` the later of the two calls wins.

`with_retries(n)` replays an attempt that got no response at all, right away:

```rust
let response = FlUrl::new("https://api.example.com/data")
    .with_retries(3) // Retry up to 3 times on failure
    .get()
    .await?;
```

`with_retry(retry_delay, amount)` is for riding out a restart of the service. While
its container is being recreated, every call for a few seconds either gets `502`/`503`
from the reverse proxy in front of it or no response at all — `with_retry` replays
both, `retry_delay` apart:

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .with_retry(Duration::from_secs(2), 15) // up to 16 attempts, 2s apart: ~30s of outage
    .get()
    .await?;

if response.get_status_code() >= 500 {
    // still down after the last attempt
}
```

| outcome of an attempt | `with_retries(n)` | `with_retry(delay, n)` |
| --- | --- | --- |
| no response: connection refused or reset, timeout (wasm: `Timeout`, `FetchError`) | replayed at once | replayed after `delay` |
| a response with a status of 500 and up | returned | replayed after `delay` |
| any other response — 2xx, 3xx, 4xx | returned | returned |

- **Idempotent methods only.** `GET`, `HEAD`, `PUT` and `DELETE` are replayed; `POST`
  and `PATCH` go out exactly once, and whatever came back — a 5xx included — is the
  result. A [streamed body](#what-does-not-apply-to-a-streamed-body) is never replayed
  either.
- **`amount` counts the attempts after the first one**, so the request goes out at most
  `amount + 1` times.
- **When the attempts run out, the last one is the result**: a 5xx comes back as
  `Ok(response)` for the caller to read the status of, a transport failure as `Err`.
- **A 5xx that gets replayed is never read** — its body is aborted before the pause. On
  native the HTTP/1 connection it came on is disposed, and an h2 stream is reset while
  the shared h2 connection stays pooled; under wasm the attempt's `AbortController` is
  aborted.
- **The pause** is `tokio::time::sleep` on native and a `setTimeout`-driven future under
  wasm. Both backends read one policy, so everything else above is the same on both.
- **A request that can not even be built** — say, a header value the browser refuses —
  is not an outage: nothing went out, so it fails at once instead of being waited on.

The time a `with_retry` call can take is the pauses plus the attempts themselves. An
upstream that refuses the connection or answers `503` straight away costs next to
nothing per attempt, so `with_retry(Duration::from_secs(2), 15)` covers about 30s of
outage. One that accepts the connection and never answers costs a whole
[request timeout](#timeouts) per attempt — on native possibly several, since
my-http-client replays a timed-out idempotent request internally before it reports
the timeout — so pair `with_retry` with a shorter `set_timeout` where that matters.

### Request Compression

```rust
let body = HttpRequestBody::as_json(&large_data);
let response = FlUrl::new("https://api.example.com/data")
    .compress() // gzips the body when it is 64 bytes or longer
    .post(body)
    .await?;
```

### Response Decompression

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .accept_gzip()
    .get()
    .await?;

let body = response.get_body_as_slice().await?; // already decompressed
```

`accept_gzip()` sends `Accept-Encoding: gzip`, unless the request already carries that
header, and decompresses a `Content-Encoding: gzip` response on the buffered reads —
`get_body_as_slice`, `get_json`, `get_body_as_str`, `receive_body`. A body read with
`get_body_as_stream` is passed on as it came. Under wasm the method does nothing: the
browser negotiates and decompresses by itself.

### Debug Request Output

```rust
let response = FlUrl::new("https://api.example.com/data")
    .print_input_request() // Prints HTTP headers to stdout
    .get()
    .await?;
```

Under wasm it writes the method and the url to the browser console instead.

### Request Debug String

Every request method has a `*_with_debug` twin that takes a `&mut String` as its last
argument and fills it with the request as it goes on the wire — verb, path and query,
headers, and the body:

```rust
let mut debug_string = String::new();
let body = HttpRequestBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/data")
    .post_with_debug(body, &mut debug_string)
    .await?;
println!("Request details: {}", debug_string);
```

The two backends lay the headers out differently. Native ends every header with a
CRLF, so the dump takes several lines:

```
[POST] PathAndQuery: '/data'; Headers: 'Content-Type: application/json
Body: {"a":1}
```

Under wasm it is one line:

```
[POST] PathAndQuery: '/data'; Headers: 'Content-Type: application/json; 'Body: {"a":1}
```

| method | debug twin |
| --- | --- |
| `get()` | `get_with_debug(&mut s)` |
| `head()` | `head_with_debug(&mut s)` |
| `delete()` | `delete_with_debug(&mut s)` |
| `post(body)` | `post_with_debug(body, &mut s)` |
| `put(body)` | `put_with_debug(body, &mut s)` |
| `patch(body)` | `patch_with_debug(body, &mut s)` |
| `execute_request(verb, model)` | `execute_request_with_debug(verb, model, &mut s)` |
| `post_request_streamed(body, len)` | `post_request_streamed_with_debug(body, len, &mut s)` |
| `put_request_streamed(body, len)` | `put_request_streamed_with_debug(body, len, &mut s)` |
| `patch_request_streamed(body, len)` | `patch_request_streamed_with_debug(body, len, &mut s)` |
| `execute_streamed(method, body, len)` | `execute_streamed_with_debug(method, body, len, &mut s)` |

The dump is written **before** compression, so `compress()` does not turn it into
gzip noise — what you read is what you sent.

The streamed variants are the one exception to "the body is in the dump": a streamed
payload exists only as it is written to the socket, so printing it would mean
buffering the very thing streaming avoids. Their dump is the request head alone.

The `IntoFlUrl` shortcuts on `&str` / `String` — `get`, `head`, `delete`, `post` and
`put`; there is no `patch` among them — carry their twins too, so
`"https://api.example.com/data".get_with_debug(&mut debug_string).await?` works as well.

Everything except the streamed methods (which are native-only) exists on both the
native and the wasm backend.

## Error Handling

```rust
use flurl::{FlUrl, FlUrlError};

match FlUrl::new("https://api.example.com/data").get().await {
    Ok(response) => {
        // A response arrived — whatever its status code
    }
    Err(FlUrlError::Timeout) => {
        // No response within `set_timeout`
    }
    Err(FlUrlError::InvalidUrl(reason)) | Err(FlUrlError::UnsupportedScheme(reason)) => {
        // The url can not be used, or this build can not serve its scheme
    }
    Err(FlUrlError::RequestBuild(reason)) => {
        // The request could not be built — nothing was sent
    }
    Err(err) => {
        // Everything else, a failed connection included
        eprintln!("Error: {}", err);
    }
}
```

- **The errors of the builder come out here too** — a url that can not be used, a
  header that must not go on the wire, a client certificate for a url that is not
  `https://`. See [Errors Come From the Request](#errors-come-from-the-request).
- **A status code is not an error.** `404` and `500` come back as `Ok(response)`;
  read `response.get_status_code()`.
- **`FlUrlError` is `#[non_exhaustive]`**, so a `match` on it needs a catch-all arm.
- **A failed connection** — refused, reset, a TLS handshake that did not go through —
  is `FlUrlError::MyHttpClientError` on native and `FlUrlError::FetchError` under
  wasm. Each variant exists on its own backend only, so code meant for both leaves
  them to the catch-all arm.
- **`err.is_timeout()`** is `true` for a timeout however it was reported.
- **Reading the body has errors of its own**: `get_json` fails with
  `FlUrlError::SerializationError` on a body that is not the expected JSON,
  `get_body_as_str` with `FlUrlError::CanNotConvertToUtf8`, any read with
  `FlUrlError::Timeout` once `set_response_body_timeout` runs out.

## Examples

### Complete Example: API Client

```rust
use flurl::{FlUrl, body::HttpRequestBody};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct User {
    id: u64,
    name: String,
    email: String,
}

async fn create_user(name: &str, email: &str) -> Result<User, Box<dyn std::error::Error>> {
    let user_data = CreateUser {
        name: name.to_string(),
        email: email.to_string(),
    };
    
    let mut response = FlUrl::new("https://api.example.com")
        .append_path_segment("users")
        .with_header("Authorization", "Bearer token123")
        .post(HttpRequestBody::as_json(&user_data))
        .await?;
    
    let user: User = response.get_json().await?;
    Ok(user)
}

async fn get_user(id: u64) -> Result<User, Box<dyn std::error::Error>> {
    let mut response = FlUrl::new("https://api.example.com")
        .append_path_segment("users")
        .append_path_segment(id.to_string())
        .with_header("Authorization", "Bearer token123")
        .get()
        .await?;
    
    let user: User = response.get_json().await?;
    Ok(user)
}
```

## Additional Notes

### Connection Reuse Details

- Connections are pooled per scheme, host, port and HTTP mode. For `https://` the TLS server name and the client certificate are part of the key too, and for a [`name@ip`](#connecting-to-a-known-ip-no-dns-native-only) url so is the ip
- An idle connection is reused for up to 120 seconds by default — see [Connection Timeout](#connection-timeout)
- At most 5 idle HTTP/1.1 connections are kept per endpoint by default; an HTTP/2 connection is one per endpoint and shared
- An expired connection is dropped the next time one is taken for the same endpoint. For endpoints that are never called again, `flurl::shared_connections_cache().gc(ttl_seconds)` sweeps the process-wide cache and `.clear()` empties it
- One process-wide cache is shared by all `FlUrl` instances, unless `set_connections_cache` gives a request a cache of its own. It is thread-safe

### Body Compression

- `compress()` is only applied if the body size is >= 64 bytes; a shorter body goes out as it is
- Uses gzip compression
- Sets the `Content-Encoding: gzip` header, unless the request already carries a `Content-Encoding`
- Compression threshold can be adjusted by modifying the source code

### HTTP Version Support

- **HTTP/2 (H2)**: Full support with multiplexing
- **HTTP/1.1 with Hyper**: Uses Hyper's HTTP/1.1 implementation
- **HTTP/1.1 without Hyper**: Uses custom HTTP/1.1 implementation (may be faster in some scenarios)

### Thread Safety

- A `FlUrl` is one request: every builder method and every method that sends takes it by value, so an instance is never shared — build a new one per request
- On native `FlUrl` is `Send + Sync` and the futures of its requests are `Send`, so a request can be built in one task and sent from another, or handed to `tokio::spawn`. Under wasm the futures are `!Send`
- Connection cache (`FlUrlHttpConnectionsCache`) is thread-safe
- Multiple async tasks can safely use different `FlUrl` instances concurrently

## License

See LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.