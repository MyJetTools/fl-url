use std::{net::IpAddr, sync::Arc, time::Duration};

use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use my_ssh::{SshAsyncChannel, SshSession};
#[cfg(feature = "_tls")]
use my_tls::{tokio_rustls::client::TlsStream, ClientCertificate};
use rust_extensions::remote_endpoint::*;

pub struct SshHttpConnector {
    pub ssh_session: Arc<SshSession>,
    pub remote_host: RemoteEndpointOwned,
    /// Set by a `scheme://server-name@ip/...` url: the ssh server is asked to
    /// connect to this ip rather than to resolve the url's host itself.
    pub resolved_ip: Option<IpAddr>,
    /// `remote_host` is the path of a unix socket on the ssh server.
    pub is_unix_socket: bool,
}

impl SshHttpConnector {
    /// `ssh:user@host:port->target`, for an error message.
    fn describe_route(&self) -> String {
        let ssh_credentials = self.ssh_session.get_ssh_credentials();
        let (ssh_host, ssh_port) = ssh_credentials.get_host_port();

        format!(
            "ssh:{}@{}:{}->{}",
            ssh_credentials.get_user_name(),
            ssh_host,
            ssh_port,
            super::describe_target(&self.remote_host, self.resolved_ip)
        )
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<SshAsyncChannel> for SshHttpConnector {
    async fn connect(&self) -> Result<SshAsyncChannel, MyHttpClientError> {
        let ssh_channel = if self.is_unix_socket {
            // A unix socket on the ssh server: a `direct-streamlocal` channel to its
            // path. A '~' in the path is resolved by my-ssh against the home of the
            // ssh user, on that machine — not against ours.
            self.ssh_session
                .open_remote_unix_stream(self.remote_host.get_host(), Duration::from_secs(30))
                .await
        } else {
            // The endpoint behind the ssh tunnel is assumed to be HTTP, so we fall
            // back to the default HTTP port when none is specified in the URL.
            let port = self
                .remote_host
                .get_port()
                .unwrap_or(crate::consts::HTTP_DEFAULT_PORT);

            let resolved_ip = self.resolved_ip.map(|ip| ip.to_string());
            let host = match resolved_ip.as_deref() {
                Some(ip) => ip,
                None => self.remote_host.get_host(),
            };

            self.ssh_session
                .open_remote_tcp_stream(host, port, Duration::from_secs(30))
                .await
        };

        match ssh_channel {
            Ok(ssh_channel) => Ok(ssh_channel),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "Can not connect to remote endpoint {}. Err:{:?}",
                    self.describe_route(),
                    err
                )),
            ),
        }
    }
    fn get_remote_endpoint<'s>(&'s self) -> RemoteEndpoint<'s> {
        self.remote_host.to_ref()
    }
    fn is_debug(&self) -> bool {
        false
    }

    fn reunite(
        read: tokio::io::ReadHalf<SshAsyncChannel>,
        write: tokio::io::WriteHalf<SshAsyncChannel>,
    ) -> SshAsyncChannel {
        read.unsplit(write)
    }
}

/// `https://` behind an ssh tunnel. The TLS session runs inside the tunnel's channel,
/// end to end with the target: the ssh server passes the bytes on and sees none of
/// them. So the server name, the client certificate and the certificate check are
/// the ones of the target, as without a tunnel.
#[cfg(feature = "_tls")]
pub struct SshHttpsConnector {
    pub ssh: SshHttpConnector,
    pub server_name: String,
    pub client_certificate: Option<ClientCertificate>,
    pub accept_invalid_certificate: bool,
    pub h2: bool,
}

#[cfg(feature = "_tls")]
#[async_trait::async_trait]
impl MyHttpClientConnector<TlsStream<SshAsyncChannel>> for SshHttpsConnector {
    async fn connect(&self) -> Result<TlsStream<SshAsyncChannel>, MyHttpClientError> {
        let ssh_channel = self.ssh.connect().await?;

        super::tls_handshake(
            ssh_channel,
            &self.ssh.describe_route(),
            &self.server_name,
            &self.client_certificate,
            self.accept_invalid_certificate,
            self.h2,
        )
        .await
    }

    fn get_remote_endpoint<'s>(&'s self) -> RemoteEndpoint<'s> {
        self.ssh.get_remote_endpoint()
    }

    fn is_debug(&self) -> bool {
        false
    }

    fn reunite(
        read: tokio::io::ReadHalf<TlsStream<SshAsyncChannel>>,
        write: tokio::io::WriteHalf<TlsStream<SshAsyncChannel>>,
    ) -> TlsStream<SshAsyncChannel> {
        read.unsplit(write)
    }
}
