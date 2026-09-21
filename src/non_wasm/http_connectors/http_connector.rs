use std::net::{IpAddr, SocketAddr};

use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use rust_extensions::remote_endpoint::{RemoteEndpoint, RemoteEndpointOwned};
use tokio::net::TcpStream;

pub struct HttpConnector {
    pub remote_host: RemoteEndpointOwned,
    pub resolved_ip: Option<IpAddr>,
}

impl HttpConnector {
    pub fn new(remote_host: RemoteEndpointOwned, resolved_ip: Option<IpAddr>) -> Self {
        Self {
            remote_host,
            resolved_ip,
        }
    }
}

/// Names the connection target in an error message: the url's `host:port`, or,
/// when the url pinned an ip (`scheme://server-name@ip/...`), the address the
/// socket really went to first — the server name was never resolved, so an error
/// naming only it would point at the wrong machine.
///
/// `91.99.167.251:443 (pinned ip for certbot.x-fine.online:443)`
pub(crate) fn describe_target(
    remote_host: &RemoteEndpointOwned,
    resolved_ip: Option<IpAddr>,
) -> String {
    let host_port = remote_host.get_host_port();

    let Some(ip) = resolved_ip else {
        return host_port.to_string();
    };

    let ip_port = match remote_host.get_port() {
        Some(port) => SocketAddr::new(ip, port).to_string(),
        None => ip.to_string(),
    };

    format!("{} (pinned ip for {})", ip_port, host_port)
}

/// Opens the TCP connection for `remote_host` — resolving its host, or, when the
/// url pinned an ip (`scheme://server-name@ip/...`), connecting to that ip on the
/// url's port with no DNS lookup at all.
pub(crate) async fn connect_tcp(
    remote_host: &RemoteEndpointOwned,
    resolved_ip: Option<IpAddr>,
) -> Result<TcpStream, MyHttpClientError> {
    let connect_result = match resolved_ip {
        Some(ip) => match remote_host.get_port() {
            Some(port) => TcpStream::connect(SocketAddr::new(ip, port)).await,
            None => {
                return Err(MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err: invalid port",
                    describe_target(remote_host, resolved_ip)
                )));
            }
        },
        None => TcpStream::connect(remote_host.get_host_port().as_str()).await,
    };

    connect_result.map_err(|err| {
        MyHttpClientError::CanNotConnectToRemoteHost(format!(
            "{}. Err:{}",
            describe_target(remote_host, resolved_ip),
            err
        ))
    })
}

#[async_trait::async_trait]
impl MyHttpClientConnector<TcpStream> for HttpConnector {
    async fn connect(&self) -> Result<TcpStream, MyHttpClientError> {
        connect_tcp(&self.remote_host, self.resolved_ip).await
    }
    fn get_remote_endpoint<'s>(&'s self) -> RemoteEndpoint<'s> {
        self.remote_host.to_ref()
    }

    fn is_debug(&self) -> bool {
        false
    }

    fn reunite(
        read: tokio::io::ReadHalf<TcpStream>,
        write: tokio::io::WriteHalf<TcpStream>,
    ) -> TcpStream {
        read.unsplit(write)
    }
}
