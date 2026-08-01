use std::time::Duration;

use my_http_client::{http1::MyHttpResponse, MyHttpClientConnector, MyHttpClientError};

use crate::non_wasm::compiled_http_request::CompiledHttpRequest;

use super::*;

pub struct MyHttpClientWrapper<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
> {
    pub key: String,
    inner: MyHttpClientWrapperInner<TStream, TConnector>,
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > MyHttpClientWrapper<TStream, TConnector>
{
    pub fn new(key: String, inner: MyHttpClientWrapperInner<TStream, TConnector>) -> Self {
        Self { key, inner }
    }

    pub fn is_h2(&self) -> bool {
        self.inner.is_h2()
    }

    pub async fn do_request(
        &self,
        request: &CompiledHttpRequest,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        self.inner.do_request(request, request_timeout).await
    }

    /// Sends a request whose body is produced as a stream. Only the `Http1Hyper`
    /// client can do this, so the caller has to have pinned the mode beforehand.
    pub async fn do_streamed_request(
        &self,
        request: my_http_client::HyperRequest,
        content_size: Option<usize>,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        self.inner
            .do_streamed_request(request, content_size, request_timeout)
            .await
    }

    pub async fn connect(&self) -> Result<(), MyHttpClientError> {
        match &self.inner {
            MyHttpClientWrapperInner::MyHttpClient(my_http_client) => {
                my_http_client.connect().await
            }
            MyHttpClientWrapperInner::Hyper(my_http_client) => my_http_client.connect().await,
            MyHttpClientWrapperInner::H2(my_http_client) => my_http_client.connect().await,
        }
    }
}
