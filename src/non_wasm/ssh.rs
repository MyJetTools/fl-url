use std::sync::Arc;

use my_ssh::*;
use rust_extensions::remote_endpoint::SshRemoteEndpoint;

pub struct SshTarget {
    pub credentials: Option<Arc<SshCredentials>>,
    pub sessions_pool: Option<Arc<SshSessionsPool>>,
    pub http_buffer_size: usize,
}

pub fn to_ssh_credentials(ssh_remote_endpoint: &SshRemoteEndpoint) -> SshCredentials {
    let (host, port) = ssh_remote_endpoint.get_host_port();
    my_ssh::SshCredentials::SshAgent {
        ssh_remote_host: host.to_string(),
        ssh_remote_port: port,
        ssh_user_name: ssh_remote_endpoint.get_user().to_string(),
    }
}
