use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};
use tokio::net::TcpStream;

use crate::{
    non_wasm::fl_url::FlUrlMode, non_wasm::http_connectors::HttpConnector, non_wasm::my_http_client_wrapper::MyHttpClientWrapper,
    ConnectionParams, FlUrlHttpConnectionsCache,
};

use super::super::HttpConnectionResolver;

pub struct HttpConnectionCreator;

impl HttpConnectionCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<TcpStream, HttpConnector>> {
        let http_connector =
            crate::non_wasm::http_connectors::HttpConnector::new(params.remote_endpoint.to_owned());

        match params.mode {
            FlUrlMode::H2 => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttp2Client::new(http_connector).into(),
            )),
            FlUrlMode::Http1NoHyper => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttpClient::new(http_connector).into(),
            )),
            FlUrlMode::Http1Hyper => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttpHyperClient::new(http_connector).into(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<TcpStream, HttpConnector> for HttpConnectionCreator {
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<TcpStream, HttpConnector>> {
        let key = super::super::utils::get_http_connection_key(params);
        Self::create_connection(params, key)
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<TcpStream, HttpConnector>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<TcpStream, HttpConnector> for FlUrlHttpConnectionsCache {
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<TcpStream, HttpConnector>> {
        self.get_http_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<TcpStream, HttpConnector>>,
    ) {
        self.put_http_connection_back_sync(connection);
    }

    async fn drop_connection(
        &self,
        connection: Arc<MyHttpClientWrapper<TcpStream, HttpConnector>>,
    ) {
        self.drop_http_connection_sync(&connection);
    }
}
