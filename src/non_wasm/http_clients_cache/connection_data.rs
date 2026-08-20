#[cfg(all(unix, feature = "with-ssh"))]
use std::sync::Arc;

#[cfg(feature = "with-ring-tls")]
use my_tls::ClientCertificate;
use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::FlUrlMode;

#[derive(Clone)]
pub struct ConnectionParams<'s> {
    pub mode: FlUrlMode,
    pub remote_endpoint: RemoteEndpoint<'s>,
    pub host_header: Option<&'s str>,
    #[cfg(feature = "with-ring-tls")]
    pub client_certificate: Option<&'s ClientCertificate>,
    pub accept_invalid_certificate: bool,
    pub reuse_connection_timeout_seconds: i64,
    #[cfg(all(unix, feature = "with-ssh"))]
    pub ssh_session: Option<Arc<my_ssh::SshSession>>,
}

#[cfg(feature = "with-ring-tls")]
impl<'s> ConnectionParams<'s> {
    pub fn get_server_name(&'s self) -> &'s str {
        let host = if let Some(host_header) = self.host_header {
            host_header
        } else {
            self.remote_endpoint.get_host()
        };

        strip_port(host)
    }
}

/// Strips a trailing `:port` from a `host:port` value so it can be used as a TLS
/// server name. Leaves IPv6 literals (multiple colons) and non-numeric suffixes
/// untouched.
#[cfg(feature = "with-ring-tls")]
fn strip_port(host: &str) -> &str {
    if let Some(idx) = host.rfind(':') {
        let before = &host[..idx];
        let after = &host[idx + 1..];
        if !before.contains(':') && !after.is_empty() && after.bytes().all(|b| b.is_ascii_digit()) {
            return before;
        }
    }

    host
}
