use std::sync::Arc;

use my_http_client::http1::MyHttpClient;
use my_tls::ClientCertificate;
use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};
use tokio::net::TcpStream;
use url_utils::UrlBuilder;

use crate::http_connectors::HttpConnector;

use super::{HttpClientResolver, HttpClientsCache};

pub struct HttpClientCreator;

const HTTP_DEFAULT_PORT: u16 = 80;

#[async_trait::async_trait]
impl HttpClientResolver<TcpStream, HttpConnector> for HttpClientCreator {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        _domain_override: Option<&String>,
        _client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<TcpStream, HttpConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into());
        let http_connector = crate::http_connectors::HttpConnector::new(remote_endpoint.to_owned());

        Arc::new(MyHttpClient::new(http_connector))
    }

    async fn drop_http_client(
        &self,
        _url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpClientResolver<TcpStream, HttpConnector> for HttpClientsCache {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<TcpStream, HttpConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into());

        let hash_map_key = get_http_key(remote_endpoint);

        let mut write_access = self.inner.write().await;

        if let Some(existing_connection) = write_access.http.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = HttpClientCreator
            .get_http_client(
                url_builder,
                domain_override,
                client_certificate,
                #[cfg(feature = "with-ssh")]
                ssh_credentials,
            )
            .await;

        write_access
            .http
            .insert(hash_map_key.to_string(), new_one.clone());

        new_one
    }

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
        let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into());
        let hash_map_key = get_http_key(remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.http.remove(hash_map_key.as_str());
    }
}

fn get_http_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port()
}
