use bytes::Bytes;

use http_body_util::Full;

pub enum CompiledHttpRequest {
    Hyper(my_http_client::http::request::Request<Full<Bytes>>),
    MyHttpClient(my_http_client::http1::MyHttpRequest),
}

impl CompiledHttpRequest {
    pub fn print_http_headers(&self) {
        match self {
            CompiledHttpRequest::Hyper(request) => {
                println!("{:?}", request.headers());
            }
            CompiledHttpRequest::MyHttpClient(my_http_request) => {
                println!(
                    "{:?}",
                    std::str::from_utf8(my_http_request.headers.as_slice())
                );
            }
        }
    }

    pub fn unwrap_as_hyper(&self) -> my_http_client::http::request::Request<Full<Bytes>> {
        match self {
            CompiledHttpRequest::Hyper(request) => {
                return request.clone();
            }
            CompiledHttpRequest::MyHttpClient(_) => {
                panic!("Can no unwrap request as hyper");
            }
        }
    }

    pub fn unwrap_as_my_http_client_request(&self) -> my_http_client::http1::MyHttpRequest {
        match self {
            CompiledHttpRequest::Hyper(_) => {
                panic!("Can no unwrap request as my_http_client");
            }
            CompiledHttpRequest::MyHttpClient(my_http_request) => {
                return my_http_request.clone();
            }
        }
    }
}
