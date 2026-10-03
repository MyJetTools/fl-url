use std::sync::Arc;

use my_http_client::{
    http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client, MyHttpClientError,
};

use crate::{
    non_wasm::fl_url::FlUrlMode, non_wasm::http_connectors::SshHttpConnector,
    non_wasm::my_http_client_wrapper::MyHttpClientWrapper, ConnectionParams, FlUrlHttpConnectionsCache,
    HttpConnectionResolver,
};

/// The connection params of an ssh tunnel carry the session to open the channel in;
/// without one there is nowhere to connect to.
pub(crate) fn no_ssh_session() -> MyHttpClientError {
    MyHttpClientError::CanNotConnectToRemoteHost(
        "Can not connect through an ssh tunnel: the connection params carry no ssh session"
            .to_string(),
    )
}

pub struct SshConnectionCreator;
impl SshConnectionCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        ssh_session: Arc<my_ssh::SshSession>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let connector = SshHttpConnector {
            ssh_session,
            remote_host: params.remote_endpoint.to_owned(),
            resolved_ip: params.resolved_ip,
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
    ) -> Result<Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>, MyHttpClientError>
    {
        let Some(ssh_session) = params.ssh_session.clone() else {
            return Err(no_ssh_session());
        };

        let key = super::super::utils::get_ssh_connection_key(
            ssh_session.get_ssh_credentials(),
            params.remote_endpoint,
            params.resolved_ip,
            params.mode,
        );
        Ok(Self::create_connection(params, ssh_session, key))
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
    ) -> Result<Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>, MyHttpClientError>
    {
        self.get_ssh_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>,
    ) {
        self.put_ssh_connection_back_sync(connection);
    }

    async fn drop_connection(
        &self,
        connection: Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>,
    ) {
        self.drop_ssh_connection_sync(&connection);
    }
}
