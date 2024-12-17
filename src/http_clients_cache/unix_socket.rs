use std::sync::Arc;

use crate::http_connectors::{UnixSocketConnector, UnixSocketStream};
use my_http_client::http1::MyHttpClient;
use my_tls::ClientCertificate;
use rust_extensions::ShortString;
use url_utils::UrlBuilder;

use super::{HttpClientResolver, HttpClientsCache};

use rust_extensions::remote_endpoint::RemoteEndpoint;

pub struct UnixSocketHttpClientCreator;

#[async_trait::async_trait]
impl HttpClientResolver<UnixSocketStream, UnixSocketConnector> for UnixSocketHttpClientCreator {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        _domain_override: Option<&String>,
        _client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<UnixSocketStream, UnixSocketConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let connector = UnixSocketConnector::new(remote_endpoint.to_owned());
        let new_one = MyHttpClient::new(connector);

        let new_one = Arc::new(new_one);
        new_one
    }

    async fn drop_http_client(
        &self,
        _url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpClientResolver<UnixSocketStream, UnixSocketConnector> for HttpClientsCache {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<UnixSocketStream, UnixSocketConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_unix_socket_key(remote_endpoint);

        if let Some(existing_connection) = write_access.unix_socket.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = UnixSocketHttpClientCreator
            .get_http_client(
                url_builder,
                domain_override,
                client_certificate,
                #[cfg(feature = "with-ssh")]
                ssh_credentials,
            )
            .await;

        write_access
            .unix_socket
            .insert(hash_map_key.to_string(), new_one.clone());

        new_one
    }

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let hash_map_key = get_unix_socket_key(remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.unix_socket.remove(hash_map_key.as_str());
    }
}

fn get_unix_socket_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    ShortString::from_str(remote_endpoint.get_host()).unwrap()
}
