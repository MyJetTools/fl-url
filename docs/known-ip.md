# Connecting to a Known IP (no DNS, native only)

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

The same form works behind an SSH tunnel (`ssh://user@host->https://domain.com@10.0.0.7:8443/`):
the ssh server is asked to connect to the ip instead of resolving the name, and the
name is still what the TLS session is opened for.

Under wasm the browser owns DNS and TLS, so the form is not available there — the
browser refuses a url with an `@` in it.
