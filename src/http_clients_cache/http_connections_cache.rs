use std::{collections::HashMap, sync::Arc};

use tokio::{net::TcpStream, sync::RwLock};

use my_tls::tokio_rustls::client::TlsStream;

use crate::{http_connectors::*, my_http_client_wrapper::MyHttpClientWrapper};

#[derive(Default)]
pub struct FlUrlHttpConnectionsCacheInner {
    pub http: HashMap<String, Arc<MyHttpClientWrapper<TcpStream, HttpConnector>>>,
    pub https: HashMap<String, Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>>,
    #[cfg(unix)]
    pub unix_socket:
        HashMap<String, Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>>>,
    #[cfg(feature = "with-ssh")]
    pub ssh: HashMap<String, Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>>,
}

pub struct FlUrlHttpConnectionsCache {
    pub inner: RwLock<FlUrlHttpConnectionsCacheInner>,
}

impl FlUrlHttpConnectionsCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(FlUrlHttpConnectionsCacheInner::default()),
        }
    }
}
