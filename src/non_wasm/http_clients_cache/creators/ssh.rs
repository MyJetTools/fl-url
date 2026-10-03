use std::sync::Arc;

use my_http_client::{
    http1::MyHttpClient, http1_hyper::MyHttpHyperClient, http2::MyHttp2Client, MyHttpClientError,
};

#[cfg(feature = "_tls")]
use my_tls::tokio_rustls::client::TlsStream;

#[cfg(feature = "_tls")]
use crate::non_wasm::http_connectors::SshHttpsConnector;
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

/// The tunnel's channel to the target: all of the connection for `http://` and a unix
/// socket, the layer under the TLS session for `https://`.
fn ssh_connector(
    params: &ConnectionParams<'_>,
    ssh_session: Arc<my_ssh::SshSession>,
) -> SshHttpConnector {
    SshHttpConnector {
        ssh_session,
        remote_host: params.remote_endpoint.to_owned(),
        resolved_ip: params.resolved_ip,
        is_unix_socket: params.is_unix_socket,
    }
}

pub struct SshConnectionCreator;
impl SshConnectionCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        ssh_session: Arc<my_ssh::SshSession>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let connector = ssh_connector(params, ssh_session);

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

#[cfg(feature = "_tls")]
pub struct SshHttpsConnectionCreator;

#[cfg(feature = "_tls")]
impl SshHttpsConnectionCreator {
    pub fn create_connection(
        params: &ConnectionParams<'_>,
        ssh_session: Arc<my_ssh::SshSession>,
        key: String,
    ) -> Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>> {
        let connector = SshHttpsConnector {
            ssh: ssh_connector(params, ssh_session),
            server_name: params.get_server_name().to_string(),
            client_certificate: params.client_certificate.map(|x| x.clone()),
            accept_invalid_certificate: params.accept_invalid_certificate,
            h2: params.mode.is_h2(),
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

#[cfg(feature = "_tls")]
#[async_trait::async_trait]
impl HttpConnectionResolver<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>
    for SshHttpsConnectionCreator
{
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Result<
        Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>>,
        MyHttpClientError,
    > {
        let Some(ssh_session) = params.ssh_session.clone() else {
            return Err(no_ssh_session());
        };

        let key = super::super::utils::get_ssh_https_connection_key(
            ssh_session.get_ssh_credentials(),
            params,
        );
        Ok(Self::create_connection(params, ssh_session, key))
    }

    async fn put_connection_back(
        &self,
        _connection: Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>>,
    ) {
    }
}

#[cfg(feature = "_tls")]
#[async_trait::async_trait]
impl HttpConnectionResolver<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>
    for FlUrlHttpConnectionsCache
{
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Result<
        Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>>,
        MyHttpClientError,
    > {
        self.get_ssh_https_connection(params).await
    }

    async fn put_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>>,
    ) {
        self.put_ssh_https_connection_back_sync(connection);
    }

    async fn drop_connection(
        &self,
        connection: Arc<MyHttpClientWrapper<TlsStream<my_ssh::SshAsyncChannel>, SshHttpsConnector>>,
    ) {
        self.drop_ssh_https_connection_sync(&connection);
    }
}
