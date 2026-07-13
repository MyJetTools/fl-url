use std::{sync::Arc, time::Duration};

use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use my_ssh::{SshAsyncChannel, SshSession};
use rust_extensions::remote_endpoint::*;

pub struct SshHttpConnector {
    pub ssh_session: Arc<SshSession>,
    pub remote_host: RemoteEndpointOwned,
}

#[async_trait::async_trait]
impl MyHttpClientConnector<SshAsyncChannel> for SshHttpConnector {
    async fn connect(&self) -> Result<SshAsyncChannel, MyHttpClientError> {
        // The endpoint behind the ssh tunnel is assumed to be HTTP, so we fall
        // back to the default HTTP port when none is specified in the URL.
        let port = self
            .remote_host
            .get_port()
            .unwrap_or(crate::consts::HTTP_DEFAULT_PORT);

        let ssh_channel = self
            .ssh_session
            .connect_to_remote_host(self.remote_host.get_host(), port, Duration::from_secs(30))
            .await;

        match ssh_channel {
            Ok(ssh_channel) => Ok(ssh_channel),
            Err(err) => {
                let ssh_credentials = self.ssh_session.get_ssh_credentials();

                let (ssh_host, ssh_port) = ssh_credentials.get_host_port();
                Err(
                    my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                        "Can not connect to remote endpoint ssh:{}@{}:{}->{}. Err:{:?}",
                        ssh_credentials.get_user_name(),
                        ssh_host,
                        ssh_port,
                        self.remote_host.get_host_port().as_str(),
                        err
                    )),
                )
            }
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
