use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};
use my_tls::tokio_rustls::client::TlsStream;

use tokio::net::TcpStream;

use crate::{
    fl_url::FlUrlMode, http_connectors::HttpsConnector, my_http_client_wrapper::MyHttpClientWrapper,
};

use super::super::*;

pub struct HttpsConnectionCreator;

impl HttpsConnectionCreator {
    pub fn create_connection(
        params: &ConnectionData<'_>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        let server_name = if let Some(server_name) = params.server_name {
            server_name.to_string()
        } else {
            params.remote_endpoint.get_host().to_string()
        };

        let connector = HttpsConnector::new(
            params.remote_endpoint.to_owned(),
            server_name,
            params.client_certificate.map(|x| x.clone()),
            params.mode.is_h2(),
        );

        match params.mode {
            FlUrlMode::H2 => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttp2Client::new(connector).into(),
            )),
            FlUrlMode::Http1NoHyper => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttpClient::new(connector).into(),
            )),
            FlUrlMode::Http1Hyper => Arc::new(MyHttpClientWrapper::new(
                key.to_string(),
                MyHttpHyperClient::new(connector).into(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<TlsStream<TcpStream>, HttpsConnector> for HttpsConnectionCreator {
    async fn get_http_connection(
        &self,
        params: &ConnectionData<'_>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        let key = super::super::utils::get_http_connection_key(params.remote_endpoint);
        Self::create_connection(params, key.to_string())
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<TlsStream<TcpStream>, HttpsConnector> for FlUrlHttpConnectionsCache {
    async fn get_http_connection(
        &self,
        params: &ConnectionData<'_>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        self.get_https_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>,
    ) {
        self.put_https_connection_back(connection).await;
    }
}
