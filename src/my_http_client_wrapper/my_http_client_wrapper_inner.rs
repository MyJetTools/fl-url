use std::time::Duration;

use my_http_client::{http1::MyHttpResponse, MyHttpClientConnector, MyHttpClientError};

use crate::compiled_http_request::CompiledHttpRequest;

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
    pub async fn do_request(
        &self,
        request: &CompiledHttpRequest,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        match self {
            Self::MyHttpClient(my_http_client) => {
                let request = request.unwrap_as_my_http_client_request();
                my_http_client.do_request(&request, request_timeout).await
            }
            Self::Hyper(my_http_client) => {
                let request = request.unwrap_as_hyper();
                let result = my_http_client.do_request(request, request_timeout).await?;

                match result {
                    my_http_client::http1_hyper::HyperHttpResponse::Response(response) => {
                        Ok(MyHttpResponse::Response(response))
                    }
                    my_http_client::http1_hyper::HyperHttpResponse::WebSocketUpgrade {
                        response,
                        web_socket: _,
                    } => Ok(MyHttpResponse::Response(response)),
                }
            }

            Self::H2(my_http_client) => {
                //let req = req.to_hyper_h2_request(is_https);

                let request = request.unwrap_as_hyper();
                let result = my_http_client.do_request(request, request_timeout).await?;
                Ok(MyHttpResponse::Response(result))
            }
        }
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapperInner<TStream, TConnector>>
    for my_http_client::http1::MyHttpClient<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapperInner<TStream, TConnector> {
        MyHttpClientWrapperInner::MyHttpClient(self)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapperInner<TStream, TConnector>>
    for my_http_client::http1_hyper::MyHttpHyperClient<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapperInner<TStream, TConnector> {
        MyHttpClientWrapperInner::Hyper(self)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapperInner<TStream, TConnector>>
    for my_http_client::http2::MyHttp2Client<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapperInner<TStream, TConnector> {
        MyHttpClientWrapperInner::H2(self)
    }
}
