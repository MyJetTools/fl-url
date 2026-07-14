use std::time::Duration;

use my_http_client::{http1::MyHttpResponse, MyHttpClientConnector, MyHttpClientError};

use crate::non_wasm::compiled_http_request::CompiledHttpRequest;

pub enum MyHttpClientWrapperInner<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
> {
    MyHttpClient(my_http_client::http1::MyHttpClient<TStream, TConnector>),
    Hyper(my_http_client::http1_hyper::MyHttpHyperClient<TStream, TConnector>),
    H2(my_http_client::http2::MyHttp2Client<TStream, TConnector>),
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > MyHttpClientWrapperInner<TStream, TConnector>
{
    pub fn is_h2(&self) -> bool {
        matches!(self, Self::H2(_))
    }

    pub async fn do_request(
        &self,
        request: &CompiledHttpRequest,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        match self {
            Self::MyHttpClient(my_http_client) => {
                let result = my_http_client
                    .do_request(request.as_my_http_client_request(), request_timeout)
                    .await?;

                match result {
                    MyHttpResponse::Response(response) => Ok(MyHttpResponse::Response(response)),
                    // fl-url does not support WebSockets: report the upgrade as an
                    // error instead of returning a fake-success response with the
                    // socket silently dropped. The upgraded connection is consumed
                    // and must not be reused.
                    MyHttpResponse::WebSocketUpgrade { disconnection, .. } => {
                        disconnection.disconnect();
                        Err(MyHttpClientError::UpgradedToWebSocket)
                    }
                }
            }
            Self::Hyper(my_http_client) => {
                let request = request.unwrap_as_hyper();
                let result = my_http_client.do_request(request, request_timeout).await?;

                match result {
                    my_http_client::http1_hyper::HyperHttpResponse::Response(response) => {
                        Ok(MyHttpResponse::Response(response))
                    }
                    my_http_client::http1_hyper::HyperHttpResponse::WebSocketUpgrade {
                        ..
                    } => Err(MyHttpClientError::UpgradedToWebSocket),
                }
            }

            Self::H2(my_http_client) => {
                let result = my_http_client
                    .do_request(request.as_hyper(), request_timeout)
                    .await?;
                Ok(MyHttpResponse::Response(result))
            }
        }
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > From<my_http_client::http1::MyHttpClient<TStream, TConnector>>
    for MyHttpClientWrapperInner<TStream, TConnector>
{
    fn from(client: my_http_client::http1::MyHttpClient<TStream, TConnector>) -> Self {
        MyHttpClientWrapperInner::MyHttpClient(client)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > From<my_http_client::http1_hyper::MyHttpHyperClient<TStream, TConnector>>
    for MyHttpClientWrapperInner<TStream, TConnector>
{
    fn from(client: my_http_client::http1_hyper::MyHttpHyperClient<TStream, TConnector>) -> Self {
        MyHttpClientWrapperInner::Hyper(client)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > From<my_http_client::http2::MyHttp2Client<TStream, TConnector>>
    for MyHttpClientWrapperInner<TStream, TConnector>
{
    fn from(client: my_http_client::http2::MyHttp2Client<TStream, TConnector>) -> Self {
        MyHttpClientWrapperInner::H2(client)
    }
}
