# Timeouts and the response body size limit

## Timeouts

```rust
use std::time::Duration;

let mut response = FlUrl::new("https://api.example.com/data")
    .set_timeout(Duration::from_secs(30))
    .set_response_body_timeout(Duration::from_secs(60))
    .get()
    .await?;

let body = response.get_body()?.into_vec(1024 * 1024).await?;
```

| method | bounds | default | when it runs out |
| --- | --- | --- | --- |
| `set_timeout` | the request up to the response head — for a [streamed body](streamed-body.md#what-does-not-apply-to-a-streamed-body) the upload as well — and, natively, the read of the whole body after it | 10 seconds | the request, or the read, fails with `FlUrlError::Timeout` |
| `set_response_body_timeout` | reading the response body: a read of the whole body as a whole, a read piece by piece for each piece | unbounded | the read fails with `FlUrlError::Timeout` |

### What bounds each way of reading the body

| the body is read with | `set_timeout` | `set_response_body_timeout` |
| --- | --- | --- |
| `into_vec`, `into_string`, `into_json` | native: yes, on the same clock the request runs on; wasm: no | the read as a whole |
| `next_item` | no | each piece |
| the body of `into_hyper_response` (native only) | no | no |

- **Natively a request whose body is read into memory is bounded as a whole.** The
  clock of `set_timeout` starts when the request is sent and does not start over for
  the body: the request, its head and the read of the whole body have to fit into the
  one timeout. A read which is called after it has run out fails, unless the body has
  arrived already. Every attempt of a [retry](retries.md) has the whole timeout.
- **A body which is read piece by piece may last for as long as it is sent** — an event
  stream does. Only `set_response_body_timeout` bounds it, and that is how long a
  single piece may be waited for.
- **Under wasm `set_timeout` ends with the response head**: reading the body is bounded
  by `set_response_body_timeout` alone.

## Response Body Size

FlUrl has no size limit of its own: the limit is given where the body is read into
memory. `into_vec`, `into_string` and `into_json` of the
[reader of the body](index_resource.md#read-the-body) each take it, in bytes:

```rust
let mut response = FlUrl::new("https://api.example.com/export")
    .get()
    .await?;

// 10 MB at most: a bigger body fails with FlUrlError::ResponseBodyTooLarge
let body = response.get_body()?.into_vec(10 * 1024 * 1024).await?;
```

- A body whose `Content-Length` is over the limit is refused without being read; one
  without it is cut off as soon as it grows past the limit. The read fails with
  `FlUrlError::ResponseBodyTooLarge { limit }`.
- The rest of the body is not downloaded. Natively an HTTP/1 connection is closed
  rather than returned to the pool, and in `FlUrlMode::H2` only that stream is
  dropped; under wasm the `fetch` is aborted.
- `usize::MAX` takes a body of any size.
- With `accept_gzip()` the limit holds the decoded body: a few kilobytes of gzip can
  inflate into gigabytes. Under wasm the browser decodes every compressed body before
  it is handed over, so there the limit holds the decoded body as well.
- A body read piece by piece with `next_item` is never held whole, and has no limit.
