use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};
use my_tls::ClientCertificate;
use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};
use url_utils::UrlBuilder;

use crate::{
    fl_url::FlUrlMode, http_connectors::SshHttpConnector,
    my_http_client_wrapper::MyHttpClientWrapper,
};

use super::{FlUrlHttpClientsCache, HttpClientResolver};

pub struct SshHttpClientCreator;

#[async_trait::async_trait]
impl HttpClientResolver<my_ssh::SshAsyncChannel, SshHttpConnector> for SshHttpClientCreator {
    async fn get_http_client(
        &self,
        mode: FlUrlMode,
        url_builder: &UrlBuilder,
        _host_header: Option<&str>,
        _client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let ssh_credentials = ssh_credentials.unwrap();
        let ssh_session = my_ssh::SSH_SESSIONS_POOL
            .get_or_create(ssh_credentials)
            .await;

        let remote_endpoint = url_builder.get_remote_endpoint(None);

        let connector = SshHttpConnector {
            ssh_session,
            remote_host: remote_endpoint.to_owned(),
        };
        match mode {
            FlUrlMode::H2 => Arc::new(MyHttp2Client::new(connector).into()),
            FlUrlMode::Http1NoHyper => Arc::new(MyHttpClient::new(connector).into()),
            FlUrlMode::Http1Hyper => Arc::new(MyHttpHyperClient::new(connector).into()),
        }
    }

    async fn drop_http_client(
        &self,
        _url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpClientResolver<my_ssh::SshAsyncChannel, SshHttpConnector> for FlUrlHttpClientsCache {
    async fn get_http_client(
        &self,
        mode: FlUrlMode,
        url_builder: &UrlBuilder,
        host_header: Option<&str>,
        client_certificate: Option<&ClientCertificate>,
        ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint(None);
        let hash_map_key = get_ssh_key(ssh_credentials.unwrap(), remote_endpoint);
        let mut write_access = self.inner.write().await;

        if let Some(existing_connection) = write_access.ssh.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = SshHttpClientCreator
            .get_http_client(
                mode,
                url_builder,
                host_header,
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
        let remote_endpoint = url_builder.get_remote_endpoint(None);
        let ssh_credentials = ssh_credentials.unwrap();
        let hash_map_key = get_ssh_key(ssh_credentials, remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.ssh.remove(hash_map_key.as_str());
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
    result.push_str(remote_endpoint.get_host_port().as_str());

    result
}
