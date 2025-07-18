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
        request: &my_http_client::http::request::Request<Full<Bytes>>,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        match self {
            MyHttpClientWrapper::MyHttpClient(my_http_client) => {
                let request = MyHttpRequest::from_hyper_request(request.clone()).await;
                my_http_client.do_request(&request, request_timeout).await
            }
            MyHttpClientWrapper::Hyper(my_http_client) => {
                let result = my_http_client
                    .do_request(request.clone(), request_timeout)
                    .await?;

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

            MyHttpClientWrapper::H2(my_http_client) => {
                //let req = req.to_hyper_h2_request(is_https);

                //println!("H2 request: {:?}", req);
                let result = my_http_client
                    .do_request(request.clone(), request_timeout)
                    .await?;
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
