use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use rust_extensions::remote_endpoint::*;
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

        let host = self.remote_host.get_host();

        // A path starting with '~' is accepted as a unix socket url, but '~' is a
        // shell convention the OS knows nothing about - it has to be resolved against
        // $HOME before it reaches connect(), otherwise it is looked up as a directory
        // literally named "~".
        let host = rust_extensions::file_utils::format_path(host);

        let connect_result = unix_socket.connect(host.as_str()).await;
        match connect_result {
            Ok(stream) => Ok(stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "Error connecting to '{}'. Err:{}",
                    self.remote_host.as_str(),
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
        read: tokio::io::ReadHalf<UnixStream>,
        write: tokio::io::WriteHalf<UnixStream>,
    ) -> UnixStream {
        read.unsplit(write)
    }
}
