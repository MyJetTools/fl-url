use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use rust_extensions::{remote_endpoint::RemoteEndpointOwned, StrOrString};
use tokio::net::TcpStream;

pub struct HttpConnector {
    pub remote_host: RemoteEndpointOwned,
}

impl HttpConnector {
    pub fn new(remote_host: RemoteEndpointOwned) -> Self {
        Self { remote_host }
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<TcpStream> for HttpConnector {
    async fn connect(&self) -> Result<TcpStream, MyHttpClientError> {
        let host_port = self.remote_host.get_host_port(Some(80));
        match TcpStream::connect(host_port.as_str()).await {
            Ok(tcp_stream) => Ok(tcp_stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    host_port, err
                )),
            ),
        }
    }
    fn get_remote_host(&self) -> StrOrString {
        self.remote_host.as_str().into()
    }
    fn is_debug(&self) -> bool {
        false
    }

    fn reunite(
        _read: tokio::io::ReadHalf<TcpStream>,
        _write: tokio::io::WriteHalf<TcpStream>,
    ) -> TcpStream {
        panic!("Would implement this if upgrade fl-url to support WebSockets")
    }
}