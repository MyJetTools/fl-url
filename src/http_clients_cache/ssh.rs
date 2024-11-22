use std::sync::Arc;

use my_http_client::http1::MyHttpClient;
use my_tls::ClientCertificate;
use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};

use crate::{http_connectors::SshHttpConnector, UrlBuilder};

use super::{HttpClientResolver, HttpClientsCache};

pub struct SshHttpClientCreator;

#[async_trait::async_trait]
impl HttpClientResolver<my_ssh::SshAsyncChannel, SshHttpConnector> for SshHttpClientCreator {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        _domain_override: Option<&String>,
        _client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let ssh_credentials = ssh_credentials.unwrap();
        let ssh_session = my_ssh::SSH_SESSIONS_POOL
            .get_or_create(ssh_credentials)
            .await;

        let remote_endpoint = url_builder.get_remote_endpoint();

        let connector = SshHttpConnector {
            ssh_session,
            remote_host: remote_endpoint.to_owned(),
        };
        let new_one = MyHttpClient::new(connector);

        Arc::new(new_one)
    }

    async fn drop_http_client(
        &self,
        _url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpClientResolver<my_ssh::SshAsyncChannel, SshHttpConnector> for HttpClientsCache {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let hash_map_key = get_ssh_key(ssh_credentials.unwrap(), remote_endpoint);
        let mut write_access = self.inner.write().await;

        if let Some(existing_connection) = write_access.ssh.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = SshHttpClientCreator
            .get_http_client(
                url_builder,
                domain_override,
                client_certificate,
                ssh_credentials,
            )
            .await;

        write_access
            .ssh
            .insert(hash_map_key.to_string(), new_one.clone());

        new_one
    }

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let ssh_credentials = ssh_credentials.unwrap();
        let hash_map_key = get_ssh_key(ssh_credentials, remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.http.remove(hash_map_key.as_str());
    }
}

fn get_ssh_key(
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
    result.push_str(remote_endpoint.get_host_port(None).as_str());

    result
}
