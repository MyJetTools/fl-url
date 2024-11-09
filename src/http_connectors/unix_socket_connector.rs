use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use rust_extensions::{remote_endpoint::RemoteEndpointOwned, StrOrString};
use tokio::net::{UnixSocket, UnixStream};

pub type UnixSocketStream = tokio::net::UnixStream;

pub struct UnixSocketConnector {
    pub remote_host: RemoteEndpointOwned,
}

impl UnixSocketConnector {
    pub fn new(remote_host: RemoteEndpointOwned) -> Self {
        Self { remote_host }
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<UnixStream> for UnixSocketConnector {
    async fn connect(&self) -> Result<UnixStream, MyHttpClientError> {
        let unix_socket = match UnixSocket::new_stream() {
            Ok(result) => result,
            Err(err) => {
                return Err(MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "Can not create UnixSocket to connection to {}. Err: {}",
                    self.remote_host.as_str(),
                    err
                )))
            }
        };

        let connect_result = unix_socket.connect(self.remote_host.get_host()).await;
        match connect_result {
            Ok(stream) => Ok(stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    self.remote_host.as_str(),
                    err
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
        _read: tokio::io::ReadHalf<UnixStream>,
        _write: tokio::io::WriteHalf<UnixStream>,
    ) -> UnixStream {
        panic!("Would implement this if upgrade fl-url to support WebSockets")
    }
}