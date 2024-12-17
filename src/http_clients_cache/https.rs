use std::sync::Arc;

use my_http_client::http1::MyHttpClient;
use my_tls::{tokio_rustls::client::TlsStream, ClientCertificate};
use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};
use tokio::net::TcpStream;
use url_utils::UrlBuilder;

use crate::http_connectors::HttpsConnector;

use super::{HttpClientResolver, HttpClientsCache};

pub struct HttpsClientCreator;

#[async_trait::async_trait]
impl HttpClientResolver<TlsStream<TcpStream>, HttpsConnector> for HttpsClientCreator {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<TlsStream<TcpStream>, HttpsConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let connector = HttpsConnector::new(
            remote_endpoint.to_owned(),
            domain_override.cloned(),
            client_certificate.map(|x| x.clone()),
        );
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
impl HttpClientResolver<TlsStream<TcpStream>, HttpsConnector> for HttpClientsCache {
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<TlsStream<TcpStream>, HttpsConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let hash_map_key = get_https_key(remote_endpoint);
        let mut write_access = self.inner.write().await;

        if let Some(existing_connection) = write_access.https.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = HttpsClientCreator
            .get_http_client(
                url_builder,
                domain_override,
                client_certificate,
                #[cfg(feature = "with-ssh")]
                ssh_credentials,
            )
            .await;

        write_access
            .https
            .insert(hash_map_key.to_string(), new_one.clone());

        new_one
    }

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) {
        let remote_endpoint = url_builder.get_remote_endpoint();
        let hash_map_key = get_https_key(remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.https.remove(hash_map_key.as_str());
    }
}

fn get_https_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port(Some(443))
}
