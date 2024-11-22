use std::{collections::HashMap, sync::Arc};

use my_http_client::http1::MyHttpClient;

use tokio::{net::TcpStream, sync::RwLock};

use my_tls::tokio_rustls::client::TlsStream;

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

    /*
    pub async fn remove(&self, url_builder: &UrlBuilder) {
        let remote_endpoint = url_builder.get_remote_endpoint();

        let mut write_access = self.inner.write().await;

        let hash_map_key = get_https_key(remote_endpoint);

        write_access.http.remove(hash_map_key.as_str());
        write_access.https.remove(hash_map_key.as_str());
        #[cfg(feature = "unix-socket")]
        write_access.unix_socket.remove(hash_map_key.as_str());

        #[cfg(feature = "with-ssh")]
        write_access.ssh.remove(hash_map_key.as_str());
    }
     */
}
