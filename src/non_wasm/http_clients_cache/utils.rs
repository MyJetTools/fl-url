use std::hash::{Hash, Hasher};

#[cfg(all(unix, feature = "with-ssh"))]
use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::ConnectionParams;

fn mode_tag(mode: crate::FlUrlMode) -> &'static str {
    match mode {
        crate::FlUrlMode::H2 => "h2",
        crate::FlUrlMode::Http1NoHyper => "h1",
        crate::FlUrlMode::Http1Hyper => "h1h",
    }
}

/// Connections are only interchangeable when both the endpoint and the way the
/// client was built match, so the key includes the mode the wrapper was created
/// with — a request compiled for one mode routed to a wrapper of another would
/// fail (see `CompiledHttpRequest::as_hyper` / `as_my_http_client_request`).
pub fn get_http_connection_key(params: &ConnectionParams<'_>) -> String {
    format!(
        "{}|{}",
        params.remote_endpoint.get_host_port().as_str(),
        mode_tag(params.mode)
    )
}

/// For HTTPS the TLS identity is baked into the connector at creation, so the
/// key also includes the SNI server name and the client certificate — otherwise
/// requests with different identities would silently share a handshake.
pub fn get_https_connection_key(params: &ConnectionParams<'_>) -> String {
    let cert_tag = match params.client_certificate {
        Some(cert) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for der in &cert.cert_chain {
                der.as_ref().hash(&mut hasher);
            }
            format!("{:016x}", hasher.finish())
        }
        None => "nocert".to_string(),
    };

    format!(
        "{}|{}|{}|{}",
        params.remote_endpoint.get_host_port().as_str(),
        mode_tag(params.mode),
        params.get_server_name(),
        cert_tag
    )
}

#[cfg(unix)]
pub fn get_unix_socket_connection_key(params: &ConnectionParams<'_>) -> String {
    format!(
        "{}|{}",
        params.remote_endpoint.get_host(),
        mode_tag(params.mode)
    )
}

#[cfg(all(unix, feature = "with-ssh"))]
pub fn get_ssh_connection_key(
    ssh_credentials: &my_ssh::SshCredentials,
    remote_endpoint: RemoteEndpoint,
    mode: crate::FlUrlMode,
) -> String {
    let (host, port) = ssh_credentials.get_host_port();

    format!(
        "{}@{}:{}->{}|{}",
        ssh_credentials.get_user_name(),
        host,
        port,
        remote_endpoint.get_host_port().as_str(),
        mode_tag(mode)
    )
}
