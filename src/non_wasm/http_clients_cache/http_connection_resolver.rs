use std::sync::Arc;

use my_http_client::{MyHttpClientConnector, MyHttpClientError};

use crate::non_wasm::my_http_client_wrapper::MyHttpClientWrapper;

use super::*;

#[async_trait::async_trait]
pub trait HttpConnectionResolver<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>: Send + Sync
{
    /// `Err` when no connection can be made out of these params at all — an ssh
    /// connection asked for with no ssh session in them. Nothing has been sent then.
    async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Result<Arc<MyHttpClientWrapper<TStream, TConnector>>, MyHttpClientError>;

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
