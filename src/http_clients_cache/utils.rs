use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};

pub fn get_http_connection_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port()
}

pub fn get_unix_socket_connection_key(remote_endpoint: RemoteEndpoint) -> String {
    remote_endpoint.get_host().to_string()
}

#[cfg(feature = "with-ssh")]
pub fn get_ssh_connection_key(
    ssh_credentials: &my_ssh::SshCredentials,
    remote_endpoint: RemoteEndpoint,
) -> ShortString {
    let mut result = ShortString::new_empty();

    result.push_str(ssh_credentials.get_user_name());
    result.push('@');

    let (host, port) = ssh_credentials.get_host_port();

    result.push_str(host);
    result.push(':');

    result.push_str(port.to_string().as_str());

    result.push_str("->");
    result.push_str(remote_endpoint.get_host_port().as_str());

    result
}
