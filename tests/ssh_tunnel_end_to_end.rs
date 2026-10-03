//! Real requests through a real ssh tunnel.
//!
//! The other ssh tests stop at the url, or at a tunnel that fails to open. Here an
//! ssh server is stood up — on russh, the crate my-ssh itself is built on — that
//! takes any password and serves `direct-tcpip` channels the way sshd does, with an
//! http server behind it. A request to `ssh://user@host:port->http://target` then has
//! to arrive at that http server, in every `FlUrlMode`.
#![cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]

use std::net::SocketAddr;
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
use tokio::net::{TcpListener, TcpStream};

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

/// What the server saw, echoed back for the test to assert on.
async fn echo(
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, std::convert::Infallible> {
    let seen = format!(
        "{}|{}|{}",
        req.method(),
        req.uri().path_and_query().map(|v| v.as_str()).unwrap_or(""),
        req.headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| req.uri().authority().map(|a| a.as_str()))
            .unwrap_or("-")
    );

    Ok(hyper::Response::new(Full::new(Bytes::from(seen))))
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

// ---- the requests -------------------------------------------------------------

fn tunnel(ssh_host: &str, ssh_port: u16, target: &str) -> FlUrl {
    FlUrl::new(format!("ssh://user@{ssh_host}:{ssh_port}->{target}"))
        .set_ssh_password("password")
        .set_timeout(Duration::from_secs(10))
}

async fn read_body(mut response: flurl::FlUrlResponse) -> (u16, String) {
    let status = response.get_status_code();
    let body = response.get_body_as_slice().await.unwrap().to_vec();
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

/// The form the README shows: the ssh host is a name. `localhost` is one, and
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
