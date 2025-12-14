use std::sync::Arc;

use my_http_client::MyHttpClientConnector;

use crate::my_http_client_wrapper::MyHttpClientWrapper;

use super::*;

#[async_trait::async_trait]
pub trait HttpConnectionResolver<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>
{
    async fn get_http_connection(
        &self,
        params: &ConnectionData<'_>,
    ) -> Arc<MyHttpClientWrapper<TStream, TConnector>>;

    async fn put_connection_back(&self, connection: Arc<MyHttpClientWrapper<TStream, TConnector>>);
}
