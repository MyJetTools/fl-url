use bytes::Bytes;

use http_body_util::Full;
use hyper::Method;

/// What `execute_with_retry` is handed. Both shapes travel the same connection-pool
/// path; they differ in whether the payload survives an attempt.
pub enum RequestToExecute {
    /// The body is in memory, so a failed attempt can be replayed.
    Compiled(CompiledHttpRequest),
    /// The body is produced as a stream and is consumed while it is being sent, so
    /// there is nothing left to replay — this one is a single shot. `request` is
    /// `None` once it has been handed to the client.
    ///
    /// `content_size` is the framing, and my-http-client is what applies it: `Some(n)`
    /// puts `Content-Length: n` on the request, `None` strips any content-length and
    /// lets it go out chunked. fl-url only carries the choice here.
    Streamed {
        request: Option<my_http_client::HyperRequest>,
        method: Method,
        content_size: Option<usize>,
    },
}

impl RequestToExecute {
    pub fn streamed(request: my_http_client::HyperRequest, content_size: Option<usize>) -> Self {
        let method = request.method().clone();
        Self::Streamed {
            request: Some(request),
            method,
            content_size,
        }
    }

    /// `true` when the payload can not be replayed, whatever `with_retries` says.
    pub fn is_streamed(&self) -> bool {
        matches!(self, Self::Streamed { .. })
    }

    pub fn method_is_idempotent(&self) -> bool {
        match self {
            Self::Compiled(request) => request.method_is_idempotent(),
            Self::Streamed { method, .. } => method.is_idempotent(),
        }
    }

    pub fn print_http_headers(&self) {
        match self {
            Self::Compiled(request) => request.print_http_headers(),
            Self::Streamed { request, .. } => {
                if let Some(request) = request {
                    println!("{:?}", request.headers());
                }
            }
        }
    }
}

pub enum CompiledHttpRequestInner {
    Hyper(my_http_client::http::request::Request<Full<Bytes>>),
    MyHttpClient(my_http_client::http1::MyHttpRequest),
}

pub struct CompiledHttpRequest {
    pub inner: CompiledHttpRequestInner,
    pub method: Method,
}

impl CompiledHttpRequest {
    pub fn new_hyper(
        request: my_http_client::http::request::Request<Full<Bytes>>,
        method: Method,
    ) -> Self {
        Self {
            inner: CompiledHttpRequestInner::Hyper(request),
            method,
        }
    }

    pub fn new_my_http_client(
        request: my_http_client::http1::MyHttpRequest,
        method: Method,
    ) -> Self {
        Self {
            inner: CompiledHttpRequestInner::MyHttpClient(request),
            method,
        }
    }

    pub fn method_is_idempotent(&self) -> bool {
        self.method.is_idempotent()
    }

    pub fn print_http_headers(&self) {
        match &self.inner {
            CompiledHttpRequestInner::Hyper(request) => {
                println!("{:?}", request.headers());
            }
            CompiledHttpRequestInner::MyHttpClient(my_http_request) => {
                println!(
                    "{:?}",
                    std::str::from_utf8(my_http_request.headers.as_slice())
                );
            }
        }
    }

    pub fn as_hyper(&self) -> &my_http_client::http::request::Request<Full<Bytes>> {
        match &self.inner {
            CompiledHttpRequestInner::Hyper(request) => request,
            CompiledHttpRequestInner::MyHttpClient(_) => {
                panic!("Can no unwrap request as hyper");
            }
        }
    }

    pub fn unwrap_as_hyper(&self) -> my_http_client::http::request::Request<Full<Bytes>> {
        self.as_hyper().clone()
    }

    pub fn as_my_http_client_request(&self) -> &my_http_client::http1::MyHttpRequest {
        match &self.inner {
            CompiledHttpRequestInner::Hyper(_) => {
                panic!("Can no unwrap request as my_http_client");
            }
            CompiledHttpRequestInner::MyHttpClient(my_http_request) => my_http_request,
        }
    }
}
