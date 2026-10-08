//! Real requests through a real ssh tunnel.
//!
//! The other ssh tests stop at the url, or at a tunnel that fails to open. Here an
//! ssh server is stood up — on russh, the crate my-ssh itself is built on — that
//! takes any password and serves `direct-tcpip` and `direct-streamlocal` channels the
//! way sshd does, with http servers behind it: on a tcp port and on a unix socket. A
//! request to `ssh://user@host:port->http://target`, or `->/path/to/socket`, then has
//! to arrive at that http server, in every `FlUrlMode`.
#![cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use flurl::{FlUrl, FlUrlMode};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use my_ssh::russh;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Server, Session};
use russh::Channel;
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

// ---- the ssh server ---------------------------------------------------------

#[derive(Clone)]
struct TunnelServer;

impl Server for TunnelServer {
    type Handler = Self;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> Self {
        self.clone()
    }
}

impl Handler for TunnelServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // A target that can not be reached gets its channel refused, which is what
        // dropping `reply` does.
        let Ok(mut target) = TcpStream::connect((host_to_connect, port_to_connect as u16)).await
        else {
            return Ok(());
        };

        reply.accept().await;

        tokio::spawn(async move {
            let mut tunnel = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut tunnel, &mut target).await;
        });

        Ok(())
    }

    /// A unix socket on the ssh server — `direct-streamlocal@openssh.com`, the way
    /// sshd serves it.
    async fn channel_open_direct_streamlocal(
        &mut self,
        channel: Channel<Msg>,
        socket_path: &str,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Ok(mut target) = UnixStream::connect(socket_path).await else {
            return Ok(());
        };

        reply.accept().await;

        tokio::spawn(async move {
            let mut tunnel = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut tunnel, &mut target).await;
        });

        Ok(())
    }
}

/// Starts the ssh server on a free loopback port and returns the port.
async fn start_ssh_server() -> u16 {
    let config = russh::server::Config {
        // Any key will do — my-ssh does not check the host key — so a fixed seed
        // keeps a random generator out of the test.
        keys: vec![PrivateKey::from(Ed25519Keypair::from_seed(&[7; 32]))],
        ..Default::default()
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let mut server = TunnelServer;
        let _ = server.run_on_socket(Arc::new(config), &listener).await;
    });

    port
}

// ---- the http server behind it ----------------------------------------------

#[derive(Clone, Copy)]
enum Proto {
    Http1,
    H2c,
}

/// What the server saw of a request: `METHOD|path?query|host`.
fn seen(req: &hyper::Request<hyper::body::Incoming>) -> String {
    format!(
        "{}|{}|{}",
        req.method(),
        req.uri().path_and_query().map(|v| v.as_str()).unwrap_or(""),
        req.headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| req.uri().authority().map(|a| a.as_str()))
            .unwrap_or("-")
    )
}

/// What the server saw, echoed back for the test to assert on.
async fn echo(
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, std::convert::Infallible> {
    Ok(hyper::Response::new(Full::new(Bytes::from(seen(&req)))))
}

/// Starts the http server on a free loopback port and returns the port.
async fn start_http_server(proto: Proto) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                match proto {
                    Proto::Http1 => {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service_fn(echo))
                            .await;
                    }
                    // Prior-knowledge h2: there is no TLS inside the tunnel, so no ALPN.
                    Proto::H2c => {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service_fn(echo))
                            .await;
                    }
                }
            });
        }
    });

    port
}

static NEXT_SOCKET: AtomicUsize = AtomicUsize::new(0);

/// Starts the http server on a unix socket of its own and returns the socket's path.
async fn start_unix_http_server(proto: Proto) -> String {
    let path = std::env::temp_dir().join(format!(
        "fl-url-ssh-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path).unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                match proto {
                    Proto::Http1 => {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service_fn(echo))
                            .await;
                    }
                    Proto::H2c => {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service_fn(echo))
                            .await;
                    }
                }
            });
        }
    });

    path.to_string_lossy().to_string()
}

// ---- the requests -------------------------------------------------------------

fn tunnel(ssh_host: &str, ssh_port: u16, target: &str) -> FlUrl {
    FlUrl::new(format!("ssh://user@{ssh_host}:{ssh_port}->{target}"))
        .set_ssh_password("password")
        .set_timeout(Duration::from_secs(10))
}

async fn read_body(mut response: flurl::FlUrlResponse) -> (u16, String) {
    let status = response.get_status_code();
    let body = response.get_body().unwrap().into_vec(usize::MAX).await.unwrap().to_vec();
    (status, String::from_utf8(body).unwrap())
}

async fn a_request_goes_through_the_tunnel(mode: FlUrlMode, proto: Proto) {
    let ssh_port = start_ssh_server().await;
    let http_port = start_http_server(proto).await;

    let response = tunnel(
        "127.0.0.1",
        ssh_port,
        &format!("http://127.0.0.1:{http_port}"),
    )
    .update_mode(mode)
    .append_path_segment("api")
    .append_query_param("x", Some("1"))
    .get()
    .await
    .unwrap();

    assert_eq!(
        read_body(response).await,
        (200, format!("GET|/api?x=1|127.0.0.1:{http_port}"))
    );
}

#[tokio::test]
async fn http1_hyper_goes_through_the_tunnel() {
    a_request_goes_through_the_tunnel(FlUrlMode::Http1Hyper, Proto::Http1).await;
}

#[tokio::test]
async fn http1_no_hyper_goes_through_the_tunnel() {
    a_request_goes_through_the_tunnel(FlUrlMode::Http1NoHyper, Proto::Http1).await;
}

#[tokio::test]
async fn h2_goes_through_the_tunnel() {
    a_request_goes_through_the_tunnel(FlUrlMode::H2, Proto::H2c).await;
}

/// The form docs/ssh.md shows: the ssh host is a name. `localhost` is one, and
/// resolving it needs no network.
#[tokio::test]
async fn a_host_name_works_as_the_ssh_host() {
    let ssh_port = start_ssh_server().await;
    let http_port = start_http_server(Proto::Http1).await;

    let response = tunnel(
        "localhost",
        ssh_port,
        &format!("http://127.0.0.1:{http_port}"),
    )
    .append_path_segment("api")
    .get()
    .await
    .unwrap();

    assert_eq!(
        read_body(response).await,
        (200, format!("GET|/api|127.0.0.1:{http_port}"))
    );
}

/// `server-name@ip` behind a tunnel: the ssh server is asked for the ip — the name
/// is under `.invalid` and would never resolve — while the name is what the http
/// server sees.
#[tokio::test]
async fn a_pinned_ip_behind_the_tunnel_keeps_the_server_name() {
    let ssh_port = start_ssh_server().await;
    let http_port = start_http_server(Proto::Http1).await;

    let response = tunnel(
        "127.0.0.1",
        ssh_port,
        &format!("http://my-service.invalid@127.0.0.1:{http_port}"),
    )
    .append_path_segment("api")
    .get()
    .await
    .unwrap();

    assert_eq!(
        read_body(response).await,
        (200, format!("GET|/api|my-service.invalid:{http_port}"))
    );
}

/// The second request finds both the ssh session and the http connection in their
/// pools — a different path from the first one, which has to open them.
#[tokio::test]
async fn requests_in_a_row_reuse_the_tunnel() {
    let ssh_port = start_ssh_server().await;
    let http_port = start_http_server(Proto::Http1).await;
    let target = format!("http://127.0.0.1:{http_port}");

    for i in 0..3 {
        let response = tunnel("127.0.0.1", ssh_port, &target)
            .append_path_segment("req")
            .append_query_param("i", Some(i.to_string().as_str()))
            .get()
            .await
            .unwrap();

        assert_eq!(
            read_body(response).await,
            (200, format!("GET|/req?i={i}|127.0.0.1:{http_port}"))
        );
    }
}

/// Nothing listens on port 1, so the ssh server refuses the channel — and that has
/// to come back as an error rather than hang until the timeout.
#[tokio::test]
async fn a_target_the_ssh_server_can_not_reach_is_an_error() {
    let ssh_port = start_ssh_server().await;

    let started = std::time::Instant::now();

    let result = tunnel("127.0.0.1", ssh_port, "http://127.0.0.1:1")
        .get()
        .await;

    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}

// ---- a unix socket on the ssh server --------------------------------------------

/// Every form of a unix socket url, behind the tunnel, in every mode: the request
/// arrives at the socket on the ssh server.
async fn a_request_reaches_a_unix_socket_behind_the_tunnel(mode: FlUrlMode, proto: Proto) {
    let ssh_port = start_ssh_server().await;
    let socket = start_unix_http_server(proto).await;

    for target in [
        format!("http+unix://{socket}"),
        format!("unix://{socket}"),
        socket.clone(),
    ] {
        let response = tunnel("127.0.0.1", ssh_port, &target)
            .update_mode(mode)
            .append_path_segment("api")
            .append_query_param("x", Some("1"))
            .get()
            .await
            .unwrap_or_else(|err| panic!("{mode:?} {target}: {err:?}"));

        let (status, body) = read_body(response).await;

        assert_eq!(status, 200, "{mode:?} {target}");
        assert!(body.starts_with("GET|/api?x=1|"), "{mode:?} {target}: {body}");
    }

    let _ = std::fs::remove_file(&socket);
}

#[tokio::test]
async fn http1_hyper_reaches_a_unix_socket_behind_the_tunnel() {
    a_request_reaches_a_unix_socket_behind_the_tunnel(FlUrlMode::Http1Hyper, Proto::Http1).await;
}

#[tokio::test]
async fn http1_no_hyper_reaches_a_unix_socket_behind_the_tunnel() {
    a_request_reaches_a_unix_socket_behind_the_tunnel(FlUrlMode::Http1NoHyper, Proto::Http1).await;
}

#[tokio::test]
async fn h2_reaches_a_unix_socket_behind_the_tunnel() {
    a_request_reaches_a_unix_socket_behind_the_tunnel(FlUrlMode::H2, Proto::H2c).await;
}

/// A socket the ssh server can not open gets its channel refused: an error, not a hang.
#[tokio::test]
async fn a_unix_socket_the_ssh_server_can_not_open_is_an_error() {
    let ssh_port = start_ssh_server().await;

    let result = tunnel("127.0.0.1", ssh_port, "http+unix:///tmp/fl-url-no-such.sock")
        .get()
        .await;

    assert!(result.is_err());
}

// ---- https behind the tunnel ------------------------------------------------------

/// The TLS session runs inside the tunnel's channel, end to end with the target: its
/// certificate is checked, and the server name and the client certificate are the
/// ones of the target. Needs a TLS provider; the requests that take the self-signed
/// certificate of the server here also need `dangerous-tls`.
#[cfg(any(feature = "with-ring-tls", feature = "with-rust-tls"))]
mod https {
    use super::*;

    use flurl::my_http_client::MyHttpClientError;
    use flurl::my_tls::tokio_rustls::rustls;
    use flurl::my_tls::tokio_rustls::TlsAcceptor;
    use flurl::FlUrlError;
    use rustls::client::danger::HandshakeSignatureValid;
    use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
    use rustls::pki_types::{CertificateDer, UnixTime};
    use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
    use rustls::{DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme};

    /// Asks for a client certificate and takes any, so the server can tell whether one
    /// came. The signatures of the handshake are still checked.
    #[derive(Debug)]
    struct AnyClientCertificate(Arc<CryptoProvider>);

    impl ClientCertVerifier for AnyClientCertificate {
        fn client_auth_mandatory(&self) -> bool {
            false
        }

        fn root_hint_subjects(&self) -> &[DistinguishedName] {
            &[]
        }

        fn verify_client_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _now: UnixTime,
        ) -> Result<ClientCertVerified, rustls::Error> {
            Ok(ClientCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    /// Starts an https server with a self-signed certificate for `localhost` on a free
    /// loopback port and returns the port. It answers with what it saw of the request,
    /// then of the handshake: `…|sni=<server name>|client-certificates=<n>`. h2 is
    /// what ALPN agrees on; with no ALPN it is HTTP/1.1.
    async fn start_https_server() -> u16 {
        flurl::my_tls::install_default_crypto_providers();
        let provider = CryptoProvider::get_default().unwrap().clone();
        let certificate = flurl::my_tls::self_signed_cert::generate("localhost").unwrap();

        let mut config = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(Arc::new(AnyClientCertificate(provider)))
            .with_single_cert(vec![certificate.certificate], certificate.private_key)
            .unwrap();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };

                let acceptor = acceptor.clone();

                tokio::spawn(async move {
                    let Ok(stream) = acceptor.accept(stream).await else {
                        return;
                    };

                    let (_, connection) = stream.get_ref();
                    let handshake = format!(
                        "sni={}|client-certificates={}",
                        connection.server_name().unwrap_or("-"),
                        connection.peer_certificates().map_or(0, |certs| certs.len())
                    );
                    let h2 = connection.alpn_protocol() == Some(b"h2".as_slice());

                    let service = service_fn(move |req| {
                        let body = format!("{}|{}", seen(&req), handshake);
                        async move {
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(Full::new(
                                Bytes::from(body),
                            )))
                        }
                    });

                    let io = TokioIo::new(stream);

                    if h2 {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service)
                            .await;
                    } else {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service)
                            .await;
                    }
                });
            }
        });

        port
    }

    /// Two requests in a row: the second one finds the connection in the pool — or,
    /// for h2, shares it.
    #[cfg(feature = "dangerous-tls")]
    async fn a_request_goes_through_the_tunnel_over_tls(mode: FlUrlMode) {
        let ssh_port = start_ssh_server().await;
        let https_port = start_https_server().await;
        let target = format!("https://localhost:{https_port}");

        for i in 0..2 {
            let response = tunnel("127.0.0.1", ssh_port, &target)
                .update_mode(mode)
                .accept_invalid_certificate()
                .append_path_segment("api")
                .append_query_param("i", Some(i.to_string().as_str()))
                .get()
                .await
                .unwrap_or_else(|err| panic!("{mode:?}: {err:?}"));

            assert_eq!(
                read_body(response).await,
                (
                    200,
                    format!(
                        "GET|/api?i={i}|localhost:{https_port}|sni=localhost|client-certificates=0"
                    )
                ),
                "{mode:?}"
            );
        }
    }

    #[cfg(feature = "dangerous-tls")]
    #[tokio::test]
    async fn http1_hyper_goes_through_the_tunnel_over_tls() {
        a_request_goes_through_the_tunnel_over_tls(FlUrlMode::Http1Hyper).await;
    }

    #[cfg(feature = "dangerous-tls")]
    #[tokio::test]
    async fn http1_no_hyper_goes_through_the_tunnel_over_tls() {
        a_request_goes_through_the_tunnel_over_tls(FlUrlMode::Http1NoHyper).await;
    }

    #[cfg(feature = "dangerous-tls")]
    #[tokio::test]
    async fn h2_goes_through_the_tunnel_over_tls() {
        a_request_goes_through_the_tunnel_over_tls(FlUrlMode::H2).await;
    }

    /// `server-name@ip`: the ssh server is asked for the ip — the name is under
    /// `.invalid` and would never resolve — and the TLS session is opened for the name.
    #[cfg(feature = "dangerous-tls")]
    #[tokio::test]
    async fn a_pinned_ip_behind_the_tunnel_keeps_the_tls_server_name() {
        let ssh_port = start_ssh_server().await;
        let https_port = start_https_server().await;

        let response = tunnel(
            "127.0.0.1",
            ssh_port,
            &format!("https://my-service.invalid@127.0.0.1:{https_port}"),
        )
        .accept_invalid_certificate()
        .append_path_segment("api")
        .get()
        .await
        .unwrap();

        assert_eq!(
            read_body(response).await,
            (
                200,
                format!(
                    "GET|/api|my-service.invalid:{https_port}|sni=my-service.invalid|client-certificates=0"
                )
            )
        );
    }

    #[cfg(feature = "dangerous-tls")]
    #[tokio::test]
    async fn a_client_certificate_goes_through_the_tunnel() {
        let ssh_port = start_ssh_server().await;
        let https_port = start_https_server().await;

        let generated = flurl::my_tls::self_signed_cert::generate("client").unwrap();
        let certificate = flurl::my_tls::ClientCertificate {
            private_key: generated.private_key,
            cert_chain: vec![generated.certificate],
        };

        let response = tunnel(
            "127.0.0.1",
            ssh_port,
            &format!("https://localhost:{https_port}"),
        )
        .accept_invalid_certificate()
        .with_client_certificate(certificate)
        .get()
        .await
        .unwrap();

        let (status, body) = read_body(response).await;
        assert_eq!(status, 200);
        assert!(body.ends_with("|sni=localhost|client-certificates=1"), "{body}");
    }

    /// With no `accept_invalid_certificate` the server's certificate is checked, and a
    /// self-signed one fails the handshake. Plain text through the tunnel would not
    /// have got that far.
    #[tokio::test]
    async fn a_certificate_the_client_does_not_trust_fails_the_handshake() {
        let ssh_port = start_ssh_server().await;
        let https_port = start_https_server().await;

        let result = tunnel(
            "127.0.0.1",
            ssh_port,
            &format!("https://localhost:{https_port}"),
        )
        .get()
        .await;

        match result {
            Err(FlUrlError::MyHttpClientError(MyHttpClientError::CanNotConnectToRemoteHost(
                message,
            ))) => assert!(
                message.starts_with(&format!("ssh:user@127.0.0.1:{ssh_port}->localhost:{https_port}. TLS handshake failed.")),
                "{message}"
            ),
            other => panic!("unexpected result {other:?}"),
        }
    }
}
