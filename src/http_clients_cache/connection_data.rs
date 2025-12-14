#[cfg(feature = "with-ssh")]
use std::sync::Arc;

use my_tls::ClientCertificate;
use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::FlUrlMode;

#[derive(Clone)]
pub struct ConnectionData<'s> {
    pub mode: FlUrlMode,
    pub remote_endpoint: RemoteEndpoint<'s>,
    pub server_name: Option<&'s str>,
    pub client_certificate: Option<&'s ClientCertificate>,
    #[cfg(feature = "with-ssh")]
    pub ssh_session: Option<Arc<my_ssh::SshSession>>,
}

impl<'s> ConnectionData<'s> {
    pub fn new(mode: FlUrlMode, remote_endpoint: RemoteEndpoint<'s>) -> Self {
        Self {
            mode,
            remote_endpoint,
            server_name: Default::default(),
            client_certificate: Default::default(),
            #[cfg(feature = "with-ssh")]
            ssh_session: Default::default(),
        }
    }
}
