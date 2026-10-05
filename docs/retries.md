# Retry Logic

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
  result. A [streamed body](streamed-body.md#what-does-not-apply-to-a-streamed-body) is never replayed
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
[request timeout](timeouts-and-limits.md#timeouts) per attempt — on native possibly several, since
my-http-client replays a timed-out idempotent request internally before it reports
the timeout — so pair `with_retry` with a shorter `set_timeout` where that matters.
