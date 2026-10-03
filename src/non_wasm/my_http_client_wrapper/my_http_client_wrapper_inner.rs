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
                let request = request
                    .as_my_http_client_request()
                    .ok_or_else(compiled_for_another_mode)?;

                let result = my_http_client.do_request(request, request_timeout).await?;

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
                let request = request
                    .as_hyper()
                    .ok_or_else(compiled_for_another_mode)?
                    .clone();

                let result = my_http_client.do_request(request, request_timeout).await?;

                // `HyperHttpResponse` gains a `WebSocketUpgrade` variant when
                // my-http-client is built with `with-websocket`. Cargo unifies
                // features per package, so another crate in the graph
                // (my-web-socket-client, my-reverse-proxy) turns it on for us too, and
                // then a request that carries a websocket handshake and gets a 101
                // comes back as one. fl-url does not do websockets: the upgrade is
                // dropped, which lets go of the connection it consumed, and the request
                // fails the way the own HTTP/1.1 arm fails it. The arm can not be gated
                // with `#[cfg(feature = "with-websocket")]` — that is a feature of a
                // foreign crate and always off here — so it is a catch-all, dead in a
                // build without the feature.
                match result {
                    my_http_client::http1_hyper::HyperHttpResponse::Response(response) => {
                        Ok(MyHttpResponse::Response(response))
                    }
                    #[allow(unreachable_patterns)]
                    _ => Err(MyHttpClientError::UpgradedToWebSocket),
                }
            }

            Self::H2(my_http_client) => {
                let request = request.as_hyper().ok_or_else(compiled_for_another_mode)?;

                let result = my_http_client.do_request(request, request_timeout).await?;
                Ok(MyHttpResponse::Response(result))
            }
        }
    }

    /// A streamed request body is a hyper HTTP/1.1 feature here: the own HTTP/1.1
    /// implementation serializes the request into one buffer, and the h2 client has
    /// no streaming entry point. `FlUrl` pins the mode to `Http1Hyper` before it gets
    /// this far, so the other two arms are a guard rather than a reachable path.
    pub async fn do_streamed_request(
        &self,
        request: my_http_client::HyperRequest,
        content_size: Option<usize>,
        request_timeout: Duration,
    ) -> Result<MyHttpResponse<TStream>, MyHttpClientError> {
        match self {
            Self::Hyper(my_http_client) => {
                let result = my_http_client
                    .do_streamed_request(request, content_size, request_timeout)
                    .await?;

                // Catch-all for the feature-unified `WebSocketUpgrade` variant -
                // see the note in `do_request`.
                match result {
                    my_http_client::http1_hyper::HyperHttpResponse::Response(response) => {
                        Ok(MyHttpResponse::Response(response))
                    }
                    #[allow(unreachable_patterns)]
                    _ => Err(MyHttpClientError::UpgradedToWebSocket),
                }
            }
            Self::MyHttpClient(_) | Self::H2(_) => {
                Err(MyHttpClientError::CanNotExecuteRequest(
                    "A streamed request body requires FlUrlMode::Http1Hyper".to_string(),
                ))
            }
        }
    }
}

/// A request compiled for one `FlUrlMode` handed to a connection of another. The
/// connection key carries the mode, so this is fl-url's own rule broken — reported
/// rather than trusted.
fn compiled_for_another_mode() -> MyHttpClientError {
    MyHttpClientError::CanNotExecuteRequest(
        "The request was compiled for another FlUrlMode than the connection it was handed to"
            .to_string(),
    )
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::Method;
    use my_http_client::{
        http1::{MyHttpClient, MyHttpRequestBuilder},
        http1_hyper::MyHttpHyperClient,
        http2::MyHttp2Client,
        MyHttpClientError,
    };
    use rust_extensions::remote_endpoint::RemoteEndpointOwned;
    use tokio::net::TcpStream;

    use super::MyHttpClientWrapperInner;
    use crate::non_wasm::compiled_http_request::CompiledHttpRequest;
    use crate::non_wasm::http_connectors::HttpConnector;

    // Never connected to: the mismatch is found before a connection is asked for.
    fn connector() -> HttpConnector {
        HttpConnector::new(
            RemoteEndpointOwned::try_parse("http://127.0.0.1:1".to_string()).unwrap(),
            None,
        )
    }

    fn own_http1_request() -> CompiledHttpRequest {
        let request = MyHttpRequestBuilder::new(Method::GET, "/").unwrap().build();
        CompiledHttpRequest::new_my_http_client(request, Method::GET)
    }

    fn hyper_request() -> CompiledHttpRequest {
        let request = hyper::Request::new(Full::new(Bytes::new()));
        CompiledHttpRequest::new_hyper(request, Method::GET)
    }

    /// The connection key carries the mode, so a request never meets a connection of
    /// another mode — and if it does, that is an error of the request, not a panic.
    #[tokio::test]
    async fn a_request_compiled_for_another_mode_is_an_error() {
        let cases: [(&str, MyHttpClientWrapperInner<TcpStream, HttpConnector>, CompiledHttpRequest); 3] = [
            ("hyper", MyHttpHyperClient::new(connector()).into(), own_http1_request()),
            ("h2", MyHttp2Client::new(connector()).into(), own_http1_request()),
            ("own http/1.1", MyHttpClient::new(connector()).into(), hyper_request()),
        ];

        for (case, client, request) in cases {
            let result = client.do_request(&request, Duration::from_secs(1)).await;

            assert!(
                matches!(result, Err(MyHttpClientError::CanNotExecuteRequest(_))),
                "{case}"
            );
        }
    }
}
