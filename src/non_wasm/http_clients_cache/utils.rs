#[cfg(feature = "_tls")]
use std::hash::{Hash, Hasher};
use std::net::IpAddr;

use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::ConnectionParams;

fn mode_tag(mode: crate::FlUrlMode) -> &'static str {
    match mode {
        crate::FlUrlMode::H2 => "h2",
        crate::FlUrlMode::Http1NoHyper => "h1",
        crate::FlUrlMode::Http1Hyper => "h1h",
    }
}

/// `host:port`, plus `@ip` when a `server-name@ip` url pinned the address — the
/// same host reached at two different ips is two different upstreams, and their
/// connections must not be handed out for each other.
fn endpoint_tag(remote_endpoint: RemoteEndpoint<'_>, resolved_ip: Option<IpAddr>) -> String {
    let host_port = remote_endpoint.get_host_port();
    match resolved_ip {
        Some(ip) => format!("{}@{}", host_port.as_str(), ip),
        None => host_port.as_str().to_string(),
    }
}

/// Connections are only interchangeable when both the endpoint and the way the
/// client was built match, so the key includes the mode the wrapper was created
/// with — a request compiled for one mode routed to a wrapper of another would
/// fail with an error (see `CompiledHttpRequest::as_hyper` / `as_my_http_client_request`).
pub fn get_http_connection_key(params: &ConnectionParams<'_>) -> String {
    format!(
        "{}|{}",
        endpoint_tag(params.remote_endpoint, params.resolved_ip),
        mode_tag(params.mode)
    )
}

/// For HTTPS the TLS identity is baked into the connector at creation, so the
/// key also includes the SNI server name and the client certificate — otherwise
/// requests with different identities would silently share a handshake.
#[cfg(feature = "_tls")]
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
        endpoint_tag(params.remote_endpoint, params.resolved_ip),
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

/// `user@host:port` of the ssh server the tunnel goes through.
#[cfg(all(unix, feature = "with-ssh"))]
fn ssh_tag(ssh_credentials: &my_ssh::SshCredentials) -> String {
    let (host, port) = ssh_credentials.get_host_port();
    format!("{}@{}:{}", ssh_credentials.get_user_name(), host, port)
}

#[cfg(all(unix, feature = "with-ssh"))]
pub fn get_ssh_connection_key(
    ssh_credentials: &my_ssh::SshCredentials,
    remote_endpoint: RemoteEndpoint,
    resolved_ip: Option<IpAddr>,
    mode: crate::FlUrlMode,
) -> String {
    format!(
        "{}->{}|{}",
        ssh_tag(ssh_credentials),
        endpoint_tag(remote_endpoint, resolved_ip),
        mode_tag(mode)
    )
}

/// `https://` behind a tunnel: the TLS identity is baked into the connector here as
/// well, so the key is the https one, behind the ssh server's.
#[cfg(all(unix, feature = "with-ssh", feature = "_tls"))]
pub fn get_ssh_https_connection_key(
    ssh_credentials: &my_ssh::SshCredentials,
    params: &ConnectionParams<'_>,
) -> String {
    format!(
        "{}->{}",
        ssh_tag(ssh_credentials),
        get_https_connection_key(params)
    )
}
