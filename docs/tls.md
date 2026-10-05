# SSL/TLS Configuration (needs a TLS provider feature)

Everything in this section requires `with-ring-tls` or `with-rust-tls`. Without one
`with_client_certificate` does not exist and an `https://` request fails with
`FlUrlError::UnsupportedScheme`.

## Accept Invalid Certificates

```rust
let response = FlUrl::new("https://self-signed.example.com")
    .accept_invalid_certificate()
    .get()
    .await?;
```

This one also needs `features = ["dangerous-tls"]` to take effect — with only
a provider feature the connection errors instead of silently dropping server-cert
verification.

## Client Certificate

```rust
use flurl::my_tls::ClientCertificate;

// A PKCS#12 file (.p12 / .pfx): the private key together with its certificate chain
let cert = ClientCertificate::from_pks12_file("client.p12", "password").await?;

let response = FlUrl::new("https://api.example.com/data")
    .with_client_certificate(cert)
    .get()
    .await?;
```

`ClientCertificate` is `my-tls`' type, re-exported as `flurl::my_tls`. It is built from
a PKCS#12 container only — there is no constructor that takes a pair of PEM files:

| constructor | reads |
| --- | --- |
| `ClientCertificate::from_pks12_file(file_name, password).await` | a file |
| `ClientCertificate::from_pkcs12(bytes, password)` | bytes already in memory |

Both return `Result<ClientCertificate, my_tls::MyTlsError>`: a file that can not be
read, a wrong password or a broken container is an error, and its message names the
reason, never the password.

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
