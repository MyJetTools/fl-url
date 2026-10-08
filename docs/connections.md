# Connections and HTTP modes

## Connection Management

### Connection Reuse

By default, FLUrl reuses connections to avoid the cost of establishing new connections and TLS handshakes. A connection is reused by a request to the same scheme, host and port, made in the same [HTTP mode](#http-modes).

```rust
let mut response1 = FlUrl::new("https://api.example.com/endpoint1")
    .get()
    .await?;

// The connection goes back to the pool once the body has been read to the end
let body = response1.get_body()?.into_vec(1024 * 1024).await?;

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
`into_vec`, `into_string` or `into_json` of its reader, by `next_item` reaching the end
of the body, or by whoever reads the body of `into_hyper_response`. Instead of going
back it is closed, and the next request opens a new one, when:

- the response has a status above 400 other than 404 — `401`, `403`, `500`, `503` and
  so on, but not `400` and not `404`;
- the response carries `Connection: close`;
- the response is dropped with its body in it, or the reader of the body is dropped
  before the end of the body;
- reading the body fails, times out, or meets a body over the limit it was given;
- the request itself fails;
- the request was made with `do_not_reuse_connection()`;
- the pool already holds its maximum of idle connections to that endpoint — 5 by
  default, `FlUrlHttpConnectionsCache::new_with_max_connections(n)` for another limit.

An HTTP/2 connection is shared instead: it stays in the pool while requests are
multiplexed over it, and leaves it when a request on it fails with anything other than
a request timeout.

Either kind is dropped once it has sat in the pool unused for longer than 120 seconds
— see [Connection Timeout](#connection-timeout) to change that.

These rules are fixed: there is no way to plug in a rule of your own.

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

### Connection Reuse Details

- Connections are pooled per scheme, host, port and HTTP mode. For `https://` the TLS server name and the client certificate are part of the key too, and for a [`name@ip`](known-ip.md) url so is the ip
- An idle connection is reused for up to 120 seconds by default — see [Connection Timeout](#connection-timeout)
- At most 5 idle HTTP/1.1 connections are kept per endpoint by default; an HTTP/2 connection is one per endpoint and shared
- An expired connection is dropped the next time one is taken for the same endpoint. For endpoints that are never called again, `flurl::shared_connections_cache().gc(ttl_seconds)` sweeps the process-wide cache and `.clear()` empties it
- One process-wide cache is shared by all `FlUrl` instances, unless `set_connections_cache` gives a request a cache of its own. It is thread-safe

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

### HTTP Version Support

- **HTTP/2 (H2)**: Full support with multiplexing
- **HTTP/1.1 with Hyper**: Uses Hyper's HTTP/1.1 implementation
- **HTTP/1.1 without Hyper**: Uses custom HTTP/1.1 implementation (may be faster in some scenarios)
