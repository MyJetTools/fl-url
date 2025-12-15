use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client};

use crate::{
    fl_url::FlUrlMode, http_connectors::SshHttpConnector,
    my_http_client_wrapper::MyHttpClientWrapper, ConnectionParams, FlUrlHttpConnectionsCache,
    HttpConnectionResolver,
};

pub struct SshConnectionCreator;
impl SshConnectionCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let Some(ssh_session) = params.ssh_session.clone() else {
            panic!("ssh_session is null");
        };

        let connector = SshHttpConnector {
            ssh_session: ssh_session.clone(),
            remote_host: params.remote_endpoint.to_owned(),
        };

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
impl HttpConnectionResolver<my_ssh::SshAsyncChannel, SshHttpConnector> for SshConnectionCreator {
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let Some(ssh_session) = params.ssh_session.clone() else {
            panic!("ssh_session is null");
        };

        let key = super::super::utils::get_ssh_connection_key(
            ssh_session.get_ssh_credentials(),
            params.remote_endpoint,
        );
        Self::create_connection(params, key.to_string())
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>,
    ) {
    }
}

#[async_trait::async_trait]
impl HttpConnectionResolver<my_ssh::SshAsyncChannel, SshHttpConnector>
    for FlUrlHttpConnectionsCache
{
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        self.get_ssh_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>,
    ) {
        self.put_ssh_connection_back(connection).await;
    }
}
