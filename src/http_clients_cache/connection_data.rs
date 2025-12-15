#[cfg(feature = "with-ssh")]
use std::sync::Arc;

use my_tls::ClientCertificate;
use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::FlUrlMode;

#[derive(Clone)]
pub struct ConnectionParams<'s> {
    pub mode: FlUrlMode,
    pub remote_endpoint: RemoteEndpoint<'s>,
    pub host_header: Option<&'s str>,
    pub client_certificate: Option<&'s ClientCertificate>,
    pub reuse_connection_timeout_seconds: i64,
    #[cfg(feature = "with-ssh")]
    pub ssh_session: Option<Arc<my_ssh::SshSession>>,
}

impl<'s> ConnectionParams<'s> {
    pub fn get_server_name(&'s self) -> &'s str {
        if let Some(host_header) = self.host_header {
            return host_header;
        }

        self.remote_endpoint.get_host()
    }
}
