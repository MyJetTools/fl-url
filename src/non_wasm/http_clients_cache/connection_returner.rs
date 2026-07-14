use std::sync::Arc;

use my_http_client::MyHttpClientConnector;

use crate::non_wasm::my_http_client_wrapper::MyHttpClientWrapper;

use super::HttpConnectionResolver;

/// Type-erased handle that owns a checked-out connection for the lifetime of a
/// response. While the handle is alive the connection is NOT in the pool, so no
/// other request can collide with the in-flight response body.
///
/// - `return_connection` puts a healthy connection back into the pool once the
///   body has been fully consumed.
/// - Dropping the handle without returning disposes the connection (an HTTP/1
///   connection with an unread body cannot be reused).
#[async_trait::async_trait]
pub trait ConnectionReturner: Send + Sync {
    async fn return_connection(self: Box<Self>);
}

pub(crate) struct PooledConnectionReturner<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
> {
    pub resolver: Arc<dyn HttpConnectionResolver<TStream, TConnector> + Send + Sync>,
    pub connection: Arc<MyHttpClientWrapper<TStream, TConnector>>,
}

#[async_trait::async_trait]
impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > ConnectionReturner for PooledConnectionReturner<TStream, TConnector>
{
    async fn return_connection(self: Box<Self>) {
        self.resolver.put_connection_back(self.connection).await;
    }
}
