use std::sync::Arc;

use crate::{
    non_wasm::fl_url::FlUrlMode,
    non_wasm::http_connectors::{UnixSocketConnector, UnixSocketStream},
    non_wasm::my_http_client_wrapper::MyHttpClientWrapper,
};
use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};

use super::super::*;

pub struct UnixSocketHttpClientCreator;

impl UnixSocketHttpClientCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>> {
        let connector = UnixSocketConnector::new(params.remote_endpoint.to_owned());

        match params.mode {
            FlUrlMode::H2 => Arc::new(MyHttpClientWrapper::new(
                key,
                MyHttp2Client::new(connector).into(),
            )),
            FlUrlMode::Http1NoHyper => Arc::new(MyHttpClientWrapper::new(
                key,
                MyHttpClient::new(connector).into(),
            )),
            FlUrlMode::Http1Hyper => Arc::new(MyHttpClientWrapper::new(
                key,
                MyHttpHyperClient::new(connector).into(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<UnixSocketStream, UnixSocketConnector> for UnixSocketHttpClientCreator {
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>> {
        let key = super::super::utils::get_unix_socket_connection_key(params);
        Self::create_connection(params, key)
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<UnixSocketStream, UnixSocketConnector> for FlUrlHttpConnectionsCache {
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>> {
        self.get_unix_socket_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>>,
    ) {
        self.put_unix_socket_connection_back_sync(connection);
    }

    async fn drop_connection(
        &self,
        connection: Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>>,
    ) {
        self.drop_unix_socket_connection_sync(&connection);
    }
}
