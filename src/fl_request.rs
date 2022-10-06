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
    #[cfg(feature = "with-native-tls")]
    async fn execute_request_https(self) -> Result<FlUrlResponse, FlUrlError> {
        let mut http_connector = hyper::client::HttpConnector::new();
        http_connector.enforce_http(false);

        let tls_conn = if let Some(client_cert) = self.fl_url.client_cert {
            native_tls::TlsConnector::builder()
                .identity(client_cert)
                .danger_accept_invalid_certs(self.fl_url.accept_invalid_certificate)
                .build()
                .unwrap()
        } else {
            native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(self.fl_url.accept_invalid_certificate)
                .build()
                .unwrap()
        };

        let tls_conn = tokio_native_tls::TlsConnector::from(tls_conn);

        let ct: hyper_tls::HttpsConnector<hyper::client::HttpConnector> =
            hyper_tls::HttpsConnector::from((http_connector, tls_conn));

        //create hyper client with https connector
        let client = hyper::Client::builder().build::<_, hyper::Body>(ct);

        let result = match client.request(self.hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(self.fl_url.url, response)),
            Err(err) => Err(err),
        };

        let result = result?;
        Ok(result)
    }

    #[cfg(not(feature = "with-native-tls"))]
    async fn execute_request_https(self) -> Result<FlUrlResponse, FlUrlError> {
        let https = hyper_tls::HttpsConnector::new();
        let client = hyper::Client::builder().build::<_, hyper::Body>(https);

        let result = match client.request(self.hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(self.fl_url.url, response)),
            Err(err) => Err(err),
        };

        let result = result?;
        Ok(result)
    }
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => Body::from(payload),
        None => Body::empty(),
    }
}
