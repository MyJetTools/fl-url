use bytes::Bytes;

use http_body_util::Full;
use hyper::Method;

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
