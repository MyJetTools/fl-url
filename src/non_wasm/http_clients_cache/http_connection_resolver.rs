use std::sync::Arc;

use my_http_client::MyHttpClientConnector;

use crate::my_http_client_wrapper::MyHttpClientWrapper;

use super::*;

#[async_trait::async_trait]
pub trait HttpConnectionResolver<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>: Send + Sync
{
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<TStream, TConnector>>;

    /// Returns a healthy connection to the pool once its response body has been
    /// fully consumed. Non-pooling resolvers drop it (which disposes it).
    async fn put_connection_back(&self, connection: Arc<MyHttpClientWrapper<TStream, TConnector>>);

    /// Reports a broken connection so pooling resolvers can evict it (relevant
    /// for shared H2 clients which stay in the pool while in use). Default: no-op;
    /// dropping the Arc disposes the connection.
    async fn drop_connection(&self, connection: Arc<MyHttpClientWrapper<TStream, TConnector>>) {
        let _ = connection;
    }
}
