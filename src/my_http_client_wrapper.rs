use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use my_http_client::{
    http1::{MyHttpRequest, MyHttpResponse},
    MyHttpClientConnector, MyHttpClientError,
};

pub enum MyHttpClientWrapper<
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
    > MyHttpClientWrapper<TStream, TConnector>
{
    pub async fn do_request(
        &self,
        req: &MyHttpRequest,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        match self {
            MyHttpClientWrapper::MyHttpClient(my_http_client) => {
                my_http_client.do_request(req, request_timeout).await
            }
            MyHttpClientWrapper::Hyper(my_http_client) => {
                let req: hyper::Request<Full<Bytes>> = req.into();
                let result = my_http_client.do_request(req, request_timeout).await?;
                Ok(MyHttpResponse::Response(result))
            }

            MyHttpClientWrapper::H2(my_http_client) => {
                let req: hyper::Request<Full<Bytes>> = req.into();
                let result = my_http_client.do_request(req, request_timeout).await?;
                Ok(MyHttpResponse::Response(result))
            }
        }
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapper<TStream, TConnector>>
    for my_http_client::http1::MyHttpClient<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapper<TStream, TConnector> {
        MyHttpClientWrapper::MyHttpClient(self)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapper<TStream, TConnector>>
    for my_http_client::http1_hyper::MyHttpHyperClient<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapper<TStream, TConnector> {
        MyHttpClientWrapper::Hyper(self)
    }
}

impl<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    > Into<MyHttpClientWrapper<TStream, TConnector>>
    for my_http_client::http2::MyHttp2Client<TStream, TConnector>
{
    fn into(self) -> MyHttpClientWrapper<TStream, TConnector> {
        MyHttpClientWrapper::H2(self)
    }
}
