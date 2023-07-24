use crate::{FlUrl, FlUrlClient, FlUrlError, UrlBuilder};
use hyper::header::*;
use hyper::{Body, Method, Request};
use rust_extensions::StrOrString;

use std::str::FromStr;

use super::FlUrlResponse;

pub struct FlRequestBuilder {
    pub url_builder: UrlBuilder,
}

impl FlRequestBuilder {
    pub fn new(url_builder: UrlBuilder) -> Self {
        Self { url_builder }
    }

    /*
    pub fn new(fl_url: &'s FlUrl, method: Method, body: Option<Vec<u8>>) -> Self {
        let url = fl_url.url.to_string();

        let mut req = Request::builder().method(method).uri(url);

        if fl_url.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in &fl_url.headers {
                let header_name = HeaderName::from_str(key).unwrap();
                headers.insert(header_name, HeaderValue::from_str(value).unwrap());
            }
        };

        Self {
            fl_url,
            hyper_request: req
                .body(Body::from(compile_body(body)))
                .expect("request builder"),
        }
    }

    pub async fn execute(self) -> Result<FlUrlResponse, FlUrlError> {
        match self.fl_url.execute_timeout {
            Some(timeout) => match tokio::time::timeout(timeout, self.execute_request()).await {
                Ok(result) => {
                    let result = result?;
                    return Ok(result);
                }
                Err(_) => {
                    return Err(FlUrlError::Timeout);
                }
            },
            None => self.execute_request().await,
        }
    }
     */
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => Body::from(payload),
        None => Body::empty(),
    }
}
