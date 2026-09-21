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

/// Opens the TCP connection for `remote_host` — resolving its host, or, when the
/// url pinned an ip (`scheme://server-name@ip/...`), connecting to that ip on the
/// url's port with no DNS lookup at all.
pub(crate) async fn connect_tcp(
    remote_host: &RemoteEndpointOwned,
    resolved_ip: Option<IpAddr>,
) -> Result<TcpStream, MyHttpClientError> {
    let host_port = remote_host.get_host_port();

    let Some(ip) = resolved_ip else {
        return TcpStream::connect(host_port.as_str()).await.map_err(|err| {
            MyHttpClientError::CanNotConnectToRemoteHost(format!("{}. Err:{}", host_port, err))
        });
    };

    let Some(port) = remote_host.get_port() else {
        return Err(MyHttpClientError::CanNotConnectToRemoteHost(format!(
            "{} ({}). Err: invalid port",
            host_port, ip
        )));
    };

    TcpStream::connect(SocketAddr::new(ip, port))
        .await
        .map_err(|err| {
            MyHttpClientError::CanNotConnectToRemoteHost(format!(
                "{} ({}). Err:{}",
                host_port, ip, err
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
