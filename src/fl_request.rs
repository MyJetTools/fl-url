use crate::{FlUrl, FlUrlError};
use hyper::header::*;
use hyper::{Body, Method, Request};

use std::str::FromStr;

use super::FlUrlResponse;

pub struct FlRequest {
    pub hyper_request: Request<Body>,
    fl_url: FlUrl,
}

impl FlRequest {
    pub fn new(fl_url: FlUrl, method: Method, body: Option<Vec<u8>>) -> Self {
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

    async fn execute_request(self) -> Result<FlUrlResponse, FlUrlError> {
        if self.fl_url.url.is_https {
            return self.execute_request_https().await;
        }

        self.execute_request_http().await
    }

    async fn execute_request_http(self) -> Result<FlUrlResponse, FlUrlError> {
        let client = hyper::Client::builder().build_http();

        let result = match client.request(self.hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(self.fl_url.url, response)),
            Err(err) => Err(err),
        };

        let result = result?;
        Ok(result)
    }

    async fn execute_request_https(self) -> Result<FlUrlResponse, FlUrlError> {
        use hyper_rustls::ConfigBuilderExt;

        if let Some(client_cert) = self.fl_url.client_cert {
            let tls = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_native_roots()
                .with_client_auth_cert(vec![client_cert.cert], client_cert.pkey)
                .unwrap();

            let https = hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls)
                .https_or_http()
                .enable_http1()
                .build();

            let client = hyper::client::Client::builder().build(https);

            let result = match client.request(self.hyper_request).await {
                Ok(response) => Ok(FlUrlResponse::new(self.fl_url.url, response)),
                Err(err) => Err(err),
            };

            return Ok(result?);
        }

        let tls = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_native_roots()
            .with_no_client_auth();

        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();

        let client = hyper::client::Client::builder().build(https);

        let result = match client.request(self.hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(self.fl_url.url, response)),
            Err(err) => Err(err),
        };

        return Ok(result?);
    }
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => Body::from(payload),
        None => Body::empty(),
    }
}
