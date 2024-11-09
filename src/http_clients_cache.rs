use std::{collections::HashMap, sync::Arc};

use my_http_client::http1::MyHttpClient;

use rust_extensions::{remote_endpoint::RemoteEndpoint, ShortString};

use tokio::{net::TcpStream, sync::RwLock};

use crate::{FlUrlError, UrlBuilder};
use my_tls::{tokio_rustls::client::TlsStream, ClientCertificate};

use crate::http_connectors::*;

#[derive(Default)]
pub struct HttpClientsCacheInner {
    pub http: HashMap<String, Arc<MyHttpClient<TcpStream, HttpConnector>>>,
    pub https: HashMap<String, Arc<MyHttpClient<TlsStream<TcpStream>, HttpsConnector>>>,
    #[cfg(feature = "unix-socket")]
    pub unix_socket: HashMap<String, Arc<MyHttpClient<UnixSocketStream, UnixSocketConnector>>>,
    #[cfg(feature = "with-ssh")]
    pub ssh: HashMap<String, Arc<MyHttpClient<my_ssh::SshAsyncChannel, SshHttpConnector>>>,
}

pub struct HttpClientsCache {
    pub inner: RwLock<HttpClientsCacheInner>,
}

impl HttpClientsCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HttpClientsCacheInner::default()),
        }
    }

    pub async fn get_http_and_reuse(
        &self,
        url_builder: &UrlBuilder,
    ) -> Result<Arc<MyHttpClient<TcpStream, HttpConnector>>, FlUrlError> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_http_key(remote_endpoint);

        if let Some(existing_connection) = write_access.http.get(hash_map_key.as_str()) {
            return Ok(existing_connection.clone());
        }

        let connector = HttpConnector::new(remote_endpoint.to_owned());

        let new_one = MyHttpClient::new(connector);

        let new_one = Arc::new(new_one);

        write_access
            .http
            .insert(hash_map_key.to_string(), new_one.clone());

        Ok(new_one)
    }

    pub async fn get_https_and_reuse(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<String>,
        client_certificate: Option<ClientCertificate>,
    ) -> Result<Arc<MyHttpClient<TlsStream<TcpStream>, HttpsConnector>>, FlUrlError> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_https_key(remote_endpoint);

        if let Some(existing_connection) = write_access.https.get(hash_map_key.as_str()) {
            return Ok(existing_connection.clone());
        }

        let connector = HttpsConnector::new(
            remote_endpoint.to_owned(),
            domain_override,
            client_certificate,
        );
        let new_one = MyHttpClient::new(connector);

        let new_one = Arc::new(new_one);

        write_access
            .https
            .insert(hash_map_key.to_string(), new_one.clone());

        Ok(new_one)
    }

    #[cfg(feature = "unix-socket")]
    pub async fn get_unix_socket_and_reuse(
        &self,
        url_builder: &UrlBuilder,
    ) -> Result<Arc<MyHttpClient<UnixSocketStream, UnixSocketConnector>>, FlUrlError> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_unix_socket_key(remote_endpoint);

        if let Some(existing_connection) = write_access.unix_socket.get(hash_map_key.as_str()) {
            return Ok(existing_connection.clone());
        }

        let connector = UnixSocketConnector::new(remote_endpoint.to_owned());
        let new_one = MyHttpClient::new(connector);

        let new_one = Arc::new(new_one);

        write_access
            .unix_socket
            .insert(hash_map_key.to_string(), new_one.clone());

        Ok(new_one)
    }

    #[cfg(feature = "with-ssh")]
    pub async fn get_ssh_and_reuse(
        &self,
        url_builder: &UrlBuilder,
        ssh_credentials: &Arc<my_ssh::SshCredentials>,
    ) -> Result<Arc<MyHttpClient<SshAsyncChannel, SshHttpConnector>>, FlUrlError> {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_ssh_key(ssh_credentials, remote_endpoint);

        if let Some(existing_connection) = write_access.ssh.get(hash_map_key.as_str()) {
            return Ok(existing_connection.clone());
        }

        let ssh_session = crate::SSH_SESSIONS_POOL
            .get_or_create(ssh_credentials)
            .await;

        let connector = SshHttpConnector {
            ssh_session,
            remote_host: remote_endpoint.to_owned(),
        };
        let new_one = MyHttpClient::new(connector);

        let new_one = Arc::new(new_one);

        write_access
            .ssh
            .insert(hash_map_key.to_string(), new_one.clone());

        Ok(new_one)
    }
}

fn get_http_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port(Some(80))
}

fn get_https_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    remote_endpoint.get_host_port(Some(443))
}

#[cfg(feature = "unix-socket")]
fn get_unix_socket_key(remote_endpoint: RemoteEndpoint) -> ShortString {
    ShortString::from_str(remote_endpoint.get_host()).unwrap()
}
#[cfg(feature = "with-ssh")]
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
