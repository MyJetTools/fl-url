use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};
use my_tls::{tokio_rustls::client::TlsStream, ClientCertificate};
use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};
use tokio::net::TcpStream;
use url_utils::UrlBuilder;

use crate::{
    fl_url::FlUrlMode, http_connectors::HttpsConnector, my_http_client_wrapper::MyHttpClientWrapper,
};

use super::{HttpClientResolver, HttpClientsCache};

pub struct HttpsClientCreator;

const HTTPS_DEFAULT_PORT: u16 = 443;

#[async_trait::async_trait]
impl HttpClientResolver<TlsStream<TcpStream>, HttpsConnector> for HttpsClientCreator {
    async fn get_http_client(
        &self,
        mode: FlUrlMode,
        url_builder: &UrlBuilder,
        host_header: Option<&str>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] _ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint(HTTPS_DEFAULT_PORT.into());

        let domain_override = if url_builder.host_is_ip() {
            match host_header {
                Some(x) => Some(x.to_string()),
                None => None,
            }
        } else {
            Some(url_builder.get_host().to_string())
        };

        let connector = HttpsConnector::new(
            remote_endpoint.to_owned(),
            domain_override,
            client_certificate.map(|x| x.clone()),
            mode.is_h2(),
        );

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
impl HttpClientResolver<TlsStream<TcpStream>, HttpsConnector> for HttpClientsCache {
    async fn get_http_client(
        &self,
        mode: FlUrlMode,
        url_builder: &UrlBuilder,
        host_header: Option<&str>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        let remote_endpoint = url_builder.get_remote_endpoint(HTTPS_DEFAULT_PORT.into());
        let hash_map_key = get_https_key(remote_endpoint);
        let mut write_access = self.inner.write().await;

        if let Some(existing_connection) = write_access.https.get(hash_map_key.as_str()) {
            return existing_connection.clone();
        }

        let new_one = HttpsClientCreator
            .get_http_client(
                mode,
                url_builder,
                host_header,
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
        let remote_endpoint = url_builder.get_remote_endpoint(HTTPS_DEFAULT_PORT.into());
        let hash_map_key = get_https_key(remote_endpoint);
        let mut write_access = self.inner.write().await;
        write_access.https.remove(hash_map_key.as_str());
    }
}

fn get_https_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port()
}
