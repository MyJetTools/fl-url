use std::sync::Arc;

use my_tls::tokio_rustls::client::TlsStream;
use tokio::net::TcpStream;

use crate::{
    http_connectors::HttpsConnector, my_http_client_wrapper::MyHttpClientWrapper, ConnectionData,
    FlUrlError, HttpConnectionResolver,
};

#[derive(Clone)]
pub struct WarmedHttpsConnection {
    inner: Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>,
}

impl WarmedHttpsConnection {
    pub fn new(params: &ConnectionData<'_>) -> Self {
        let key = super::utils::get_http_connection_key(params.remote_endpoint);
        let connection =
            super::creators::HttpsConnectionCreator::create_connection(params, key.to_string());

        Self { inner: connection }
    }

    pub async fn connect(&self) -> Result<(), FlUrlError> {
        self.inner.connect().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<TlsStream<TcpStream>, HttpsConnector> for WarmedHttpsConnection {
    async fn get_http_connection(
        &self,
        _params: &ConnectionData<'_>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        self.inner.clone()
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>,
    ) {
    }
}
