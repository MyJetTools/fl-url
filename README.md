# FLUrl

A fluent, async HTTP client library for Rust, inspired by the .NET Flurl library (https://flurl.dev/).

FLUrl is a Hyper-based HTTP client that provides a fluent API for building and executing HTTP requests with connection pooling, retry logic, and comprehensive body type support.

## Features

- **Fluent API**: Chain methods to build requests naturally
- **Connection Reuse**: Automatic connection pooling and reuse for HTTP/1.1 and HTTP/2
- **Multiple HTTP Modes**: Support for HTTP/2, HTTP/1.1 with Hyper, and HTTP/1.1 without Hyper
- **Body Types**: JSON, URL-encoded, multipart/form-data, and raw data
- **SSL/TLS**: Opt-in via the `with-tls` feature — client certificate support and invalid certificate acceptance. Without it the crate never links rustls and `https://` panics
- **SSH Tunneling**: Optional SSH tunnel support via `with-ssh` feature
- **Unix Socket Support**: Native Unix socket support (Unix systems only)
- **Retry Logic**: Configurable retry mechanism
- **Request Compression**: Automatic gzip compression for request bodies
- **Streaming Responses**: Support for streaming response bodies (native only)
- **Streaming Request Bodies**: Send a body of any size at constant memory, framed with `Content-Length` or chunked (native only) — see [Streamed Body](#streamed-body-native-only)
- **Debug Support**: Built-in request debugging capabilities
- **WASM Support**: The same API compiles to `wasm32-unknown-unknown` (browser / web-worker) on top of the `fetch` API — see [WebAssembly (WASM) Support](#webassembly-wasm-support)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
flurl = "0.6.1"
```

**`https://` needs the `with-tls` feature.** It is off by default so that a
project doing plain HTTP (or unix sockets, or SSH tunnels) does not pay for the
rustls stack — `my-tls`, `rustls`, `tokio-rustls`, `ring` and the C build of
`aws-lc-sys` all leave the dependency tree. A build without it that requests an
`https://` url **panics** at execute time with
`FlUrl does not support https: it is compiled without the 'with-tls' feature`.

```toml
[dependencies]
flurl = { version = "0.6.1", features = ["with-tls"] }
```

For SSH tunneling support:

```toml
[dependencies]
flurl = { version = "0.6.1", features = ["with-ssh"] }
```

### Feature flags

| Feature | Default | What it does |
| --- | --- | --- |
| `with-tls` | off | Links the TLS stack and enables `https://` plus [`with_client_certificate`](#client-certificate). Without it `https://` panics. |
| `dangerous-tls` | off | Makes [`accept_invalid_certificate()`](#accept-invalid-certificates) actually skip server-cert verification. Implies `with-tls`. Without it that call errors at connect time rather than silently downgrading security. |
| `with-ssh` | off | [SSH tunneling](#ssh-tunneling-with-ssh-feature) (`ssh://…->http://…` urls). Unix only. |

On `wasm32` TLS is the browser's job, so `with-tls` is irrelevant there — the
`fetch` backend handles `https://` with or without it.

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
| non-wasm | [`flurl::non_wasm`] — full hyper/tokio impl | HTTP/1.1 & HTTP/2, TLS, client certs, connection pooling, unix sockets, SSH |
| `wasm32-unknown-unknown` | [`flurl::wasm`] | the browser `fetch` API via `web-sys` |

Both backends alias their types to the crate root, and the shared pieces
(`FlUrlError`, the request `body` types, the drop-connection scenario) live at the
root and are used by both. Native-only dependencies (hyper, tokio, my-tls, …) are
excluded from the wasm build; the wasm build pulls only `web-sys` / `wasm-bindgen`.

Add it to a wasm project exactly like a normal dependency (no extra feature
needed — the target is detected automatically):

```toml
[dependencies]
flurl = "0.7"
```

### What differs under wasm

Because the browser owns the connection pool, TLS and redirects, the following
native knobs are kept for signature parity but are **no-ops** under wasm:
`set_connections_cache`, `accept_invalid_certificate`, `do_not_reuse_connection`,
`update_mode`, `accept_gzip` (the browser decompresses transparently),
`set_not_used_connection_timeout`.

These **do** work under wasm: `set_timeout` bounds the request→headers round-trip
via `AbortController` + `setTimeout`; `set_response_body_timeout` bounds the body
read on the same signal (unbounded by default, as on native); `with_retries`
replays idempotent methods only; `compress` gzips the request body.

Native-only surface that is **not available** under wasm (browsers can't express
it): `with_client_certificate` (native + `with-tls`), all `*_ssh_*` methods, unix-socket URLs,
`get_body_as_stream` / `FlResponseAsStream`, and `into_hyper_response`.

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

### Error Handling for URL Creation

```rust
use flurl::{FlUrl, FlUrlError};

// new() panics on invalid URL
let response = FlUrl::new("http://mywebsite.com").get().await?;

// try_new() returns Result for error handling
match FlUrl::try_new("invalid-url") {
    Ok(fl_url) => {
        // Use fl_url
    }
    Err(FlUrlError::InvalidUrl(e)) => {
        eprintln!("Invalid URL: {}", e);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

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
    .append_query_param("sort", None) // Adds parameter without value
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
    .append_form_data_file("file", "document.pdf", "application/pdf", file_bytes);

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
`Body` implementation works just as well.

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
| `with_retries(n)` | ignored: the payload is consumed as it is sent, so the request is attempted **exactly once**. Rebuilding the stream and calling again is the caller's decision — it owns the source data |
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

```rust
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

### Get Headers

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

// Get specific header
let content_type = response.get_header("Content-Type")?;

// Get header case-insensitive
let content_type = response.get_header_case_insensitive("content-type")?;

// Get all headers
let headers = response.get_headers();
for (key, value) in headers {
    println!("{}: {:?}", key, value);
}
```

## Connection Management

### Connection Reuse

By default, FLUrl reuses connections based on schema+domain to avoid the cost of establishing new connections and TLS handshakes.

```rust
// Connection will be reused for subsequent requests to the same domain
let response1 = FlUrl::new("https://api.example.com/endpoint1")
    .get()
    .await?;

let response2 = FlUrl::new("https://api.example.com/endpoint2")
    .get()
    .await?; // Reuses connection from response1
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

### Drop Connection Scenarios

Implement custom logic to determine when connections should be dropped:

```rust
use flurl::{DropConnectionScenario, FlUrlResponse};

pub struct MyCustomDropConnectionScenario;

impl DropConnectionScenario for MyCustomDropConnectionScenario {
    fn should_we_drop_it(&self, result: &FlUrlResponse) -> bool {
        let status_code = result.get_status_code();
        
        // Drop connection on server errors (5xx) except 500
        if status_code >= 500 && status_code != 500 {
            return true;
        }
        
        // Drop connection on specific client errors
        if status_code == 401 || status_code == 403 {
            return true;
        }
        
        false
    }
}

// Note: override_drop_connection_scenario method needs to be implemented
// in the FlUrl struct if not already present
```

The default drop connection scenario drops connections on:
- Status codes > 400 (except 404)
- Status code 499

**Note**: The connection is automatically dropped and reestablished if:
- There is a Hyper error
- The response matches the drop connection scenario criteria
- The connection hasn't been used for more than the configured timeout (default: 30 seconds)

## HTTP Modes

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

## SSL/TLS Configuration (with-tls feature)

Everything in this section requires `features = ["with-tls"]`. Without it
`with_client_certificate` does not exist and an `https://` request panics.

### Accept Invalid Certificates

```rust
let response = FlUrl::new("https://self-signed.example.com")
    .accept_invalid_certificate()
    .get()
    .await?;
```

This one also needs `features = ["dangerous-tls"]` to take effect — with only
`with-tls` the connection errors instead of silently dropping server-cert
verification.

### Client Certificate

```rust
use my_tls::ClientCertificate;

let cert = ClientCertificate::from_pem_files(
    "client.crt",
    "client.key"
)?;

let response = FlUrl::new("https://api.example.com/data")
    .with_client_certificate(cert)
    .get()
    .await?;
```

## SSH Tunneling (with-ssh feature)

### Basic SSH Tunnel

```rust
// Format: ssh://user@host:port->http://target-host:port
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .get()
    .await?;
```

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

```rust
use std::sync::Arc;
use my_ssh::ssh_settings::SshSecurityCredentialsResolver;

struct MySshResolver;

#[async_trait::async_trait]
impl SshSecurityCredentialsResolver for MySshResolver {
    async fn update_credentials(
        &self,
        credentials: &my_ssh::SshCredentials,
    ) -> my_ssh::SshCredentials {
        // Custom logic to update credentials
        credentials.clone()
    }
}

let resolver = Arc::new(MySshResolver);
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_security_credentials_resolver(resolver)
    .get()
    .await?;
```

## Unix Socket Support (Unix systems only)

```rust
let response = FlUrl::new("http+unix:///var/run/docker.sock")
    .append_path_segment("containers")
    .append_path_segment("json")
    .get()
    .await?;
```

## Advanced Configuration

### Timeouts

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .set_timeout(Duration::from_secs(30))
    .get()
    .await?;
```

### Connection Timeout

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .set_not_used_connection_timeout(Duration::from_secs(60))
    .get()
    .await?;
```

### Retry Logic

```rust
let response = FlUrl::new("https://api.example.com/data")
    .with_retries(3) // Retry up to 3 times on failure
    .get()
    .await?;
```

### Request Compression

```rust
let body = HttpRequestBody::as_json(&large_data);
let response = FlUrl::new("https://api.example.com/data")
    .compress() // Automatically gzip compress body if > 64 bytes
    .post(body)
    .await?;
```

### Debug Request Output

```rust
let response = FlUrl::new("https://api.example.com/data")
    .print_input_request() // Prints HTTP headers to stdout
    .get()
    .await?;
```

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
// [POST] PathAndQuery: '/data'; Headers: 'Content-Type: application/json; 'Body: {"a":1}
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

The `IntoFlUrl` shortcuts on `&str` / `String` carry the same twins, so
`"https://api.example.com/data".get_with_debug(&mut debug_string).await?` works too.

Everything except the streamed methods (which are native-only) exists on both the
native and the wasm backend.

## Error Handling

```rust
use flurl::{FlUrl, FlUrlError};

match FlUrl::new("https://api.example.com/data").get().await {
    Ok(response) => {
        // Handle success
    }
    Err(FlUrlError::Timeout) => {
        // Handle timeout
    }
    Err(FlUrlError::HyperError(e)) => {
        // Handle Hyper error
        if e.is_canceled() {
            // Request was canceled
        }
    }
    Err(FlUrlError::SerializationError(e)) => {
        // Handle JSON serialization error
    }
    Err(e) => {
        // Handle other errors
        eprintln!("Error: {}", e.to_string());
    }
}
```

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

- Connections are cached and reused based on `schema + domain + port`
- Default connection reuse timeout: 120 seconds
- Default unused connection timeout: 30 seconds
- Connections are automatically cleaned up when not used
- Each connection cache is thread-safe and shared across all `FlUrl` instances (unless a custom cache is provided)

### Body Compression

- Compression is only applied if the body size is >= 64 bytes
- Uses gzip compression
- Automatically sets `Content-Encoding: gzip` header
- Compression threshold can be adjusted by modifying the source code

### HTTP Version Support

- **HTTP/2 (H2)**: Full support with multiplexing
- **HTTP/1.1 with Hyper**: Uses Hyper's HTTP/1.1 implementation
- **HTTP/1.1 without Hyper**: Uses custom HTTP/1.1 implementation (may be faster in some scenarios)

### Thread Safety

- `FlUrl` instances are not thread-safe (use `Send` but not `Sync`)
- Connection cache (`FlUrlHttpConnectionsCache`) is thread-safe
- Multiple async tasks can safely use different `FlUrl` instances concurrently

## License

See LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.