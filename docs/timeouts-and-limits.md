# Timeouts and the response body size limit

## Timeouts

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
| `set_timeout` | the request up to the response head — for a [streamed body](streamed-body.md#what-does-not-apply-to-a-streamed-body) the upload as well | 10 seconds | the request fails with `FlUrlError::Timeout` |
| `set_response_body_timeout` | reading the response body: a buffered read as a whole, a stream chunk by chunk | unbounded | the read fails with `FlUrlError::Timeout` |

## Response Body Size

A buffered read — `get_body_as_slice`, `get_json`, `get_body_as_str`, `receive_body` —
holds the whole body in memory, so it accepts at most 100 MB
(`flurl::DEFAULT_MAX_RESPONSE_BODY_SIZE`). A bigger body fails the read with
`FlUrlError::ResponseBodyTooLarge { limit }`.

```rust
let mut response = FlUrl::new("https://api.example.com/export")
    .set_max_response_body_size(1024 * 1024 * 1024) // usize::MAX lifts the limit
    .get()
    .await?;

let body = response.get_body_as_slice().await?;
```

- A body whose `Content-Length` is over the limit is refused without being read; one
  without it is cut off as soon as it grows past the limit. The rest of the body is
  left unread: an HTTP/1 connection is closed rather than returned to the pool, in
  `FlUrlMode::H2` only that stream is dropped.
- With `accept_gzip()` the decoded body is held to the same limit.
- In `FlUrlMode::Http1NoHyper` a body with `Content-Length` has already been read whole
  by my-http-client (up to its own 100 MB cap) by the time the response arrives: the
  limit still fails the read, but the body has been in memory. The other modes read it
  as it comes.
- Under wasm the browser downloads the body before it can be measured, unless an
  uncompressed response announces it in `Content-Length`; the limit keeps it out of
  wasm memory.
- A streamed body (`get_body_as_stream`) is never held whole and is not limited.
