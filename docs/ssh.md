# SSH Tunneling (with-ssh feature)

## Basic SSH Tunnel

```rust
// Format: ssh://user@host:port->http://target-host:port
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .get()
    .await?;

// The same through TLS: the session runs inside the tunnel
let response = FlUrl::new("ssh://user@ssh.example.com:22->https://internal.example.com/api/data")
    .get()
    .await?;
```

Behind the tunnel a url means what it means without one:

- `http://`, or no scheme at all: plain HTTP to the target, which the ssh server
  connects to;
- `https://`: the TLS session runs inside the tunnel, end to end with the target, and
  the ssh server only passes the bytes on. The certificate check, the server name,
  `with_client_certificate` and `accept_invalid_certificate` are those of the target,
  and a TLS provider feature is needed, as without a tunnel;
- a unix socket on the ssh server: see
  [Unix Socket on the SSH Server](#unix-socket-on-the-ssh-server).

`ws://` and `wss://` fail the request with `FlUrlError::UnsupportedScheme`, as they do
without a tunnel.

With no credentials given, the session is opened with the keys of the running ssh
agent (`$SSH_AUTH_SOCK`). The methods below give it a password or a private key
instead. On a url that is not an ssh tunnel they do nothing, so the same code works
whether settings name `ssh://…->http://…` or a plain `http://…`.

## SSH with Password

```rust
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_password("password123")
    .get()
    .await?;
```

## SSH with Private Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, None) // None = no passphrase
    .get()
    .await?;
```

## SSH with Passphrase-Protected Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, Some("passphrase".to_string()))
    .get()
    .await?;
```

## SSH Credentials Resolver

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

## Unix Socket on the SSH Server

After the `->` a unix socket is named in any of the forms
[Unix Socket Support](unix-socket.md) lists:

```rust
let response = FlUrl::new("ssh://user@ssh.example.com:22->/var/run/docker.sock")
    .append_path_segment("containers")
    .append_path_segment("json")
    .get()
    .await?;
```

The ssh server opens the socket, so the path is a path on that machine:

- `~` in it is the home of the ssh user there. It is read by running `echo $HOME` over
  the session, so an account that may only forward can not use `~`; an absolute path
  works without it.
- The ssh user needs access to the socket file, and sshd has to allow forwarding to a
  socket: `AllowStreamLocalForwarding`, which is on by default. A socket the server
  refuses fails the request with an error.
