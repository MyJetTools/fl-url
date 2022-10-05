use crate::{FlUrl, FlUrlError, FlUrlUriBuilder};
use hyper::header::*;
use hyper::{Body, Method, Request};

use std::str::FromStr;
use std::time::Duration;

use native_tls::Identity;

use super::FlUrlResponse;

pub struct FlRequest {
    pub hyper_request: Request<Body>,
}

impl FlRequest {
    pub fn new(fl_url: &mut FlUrl, method: Method, body: Option<Vec<u8>>) -> Self {
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
            hyper_request: req
                .body(Body::from(compile_body(body)))
                .expect("request builder"),
        }
    }

    pub async fn execute(
        self,
        url: FlUrlUriBuilder,
        execute_timeout: Option<Duration>,
        certificate: Option<Identity>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        match execute_timeout {
            Some(timeout) => {
                match tokio::time::timeout(
                    timeout,
                    execute_request(url, self.hyper_request, certificate),
                )
                .await
                {
                    Ok(result) => {
                        let result = result?;
                        return Ok(result);
                    }
                    Err(_) => {
                        return Err(FlUrlError::Timeout);
                    }
                }
            }
            None => execute_request(url, self.hyper_request, certificate).await,
        }
    }
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => Body::from(payload),
        None => Body::empty(),
    }
}

async fn execute_request(
    url: FlUrlUriBuilder,
    hyper_request: Request<Body>,
    certificate: Option<Identity>,
) -> Result<FlUrlResponse, FlUrlError> {
    if url.is_https {
        return execute_request_https(url, hyper_request, certificate).await;
    }

    execute_request_http(url, hyper_request).await
}

async fn execute_request_https(
    url: FlUrlUriBuilder,
    hyper_request: Request<Body>,
    certificate: Option<Identity>,
) -> Result<FlUrlResponse, FlUrlError> {
    if let Some(certificate) = certificate {
        let mut http_connector = hyper::client::HttpConnector::new();
        http_connector.enforce_http(false);

        let tls_conn = native_tls::TlsConnector::builder()
            .identity(certificate)
            .build()
            .unwrap();

        let tls_conn = tokio_native_tls::TlsConnector::from(tls_conn);

        let ct: hyper_tls::HttpsConnector<hyper::client::HttpConnector> =
            hyper_tls::HttpsConnector::from((http_connector, tls_conn));

        //create hyper client with https connector
        let client = hyper::Client::builder().build::<_, hyper::Body>(ct);

        let result = match client.request(hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(url, response)),
            Err(err) => Err(err),
        };

        let result = result?;
        Ok(result)
    } else {
        let https_connector = hyper_tls::HttpsConnector::new();
        let client = hyper::Client::builder().build::<_, hyper::Body>(https_connector);
        let result = match client.request(hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(url, response)),
            Err(err) => Err(err),
        };

        let result = result?;
        Ok(result)
    }
}

async fn execute_request_http(
    url: FlUrlUriBuilder,
    hyper_request: Request<Body>,
) -> Result<FlUrlResponse, FlUrlError> {
    let client = hyper::Client::builder().build_http();

    let result = match client.request(hyper_request).await {
        Ok(response) => Ok(FlUrlResponse::new(url, response)),
        Err(err) => Err(err),
    };

    let result = result?;
    Ok(result)
}
