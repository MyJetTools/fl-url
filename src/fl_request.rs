use hyper::header::*;
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use std::str::FromStr;
use std::time::Duration;

use crate::telemetry_flow::TelemetryFlow;
use crate::{FlUrl, FlUrlError, FlUrlUriBuilder};

use super::FlUrlResponse;

pub struct FlRequest {
    pub hyper_request: Request<Body>,
}

impl FlRequest {
    pub fn new(fl_url: &FlUrl, method: Method, body: Option<Vec<u8>>) -> Self {
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
        telemetry_flow: Option<TelemetryFlow>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        match execute_timeout {
            Some(timeout) => match tokio::time::timeout(
                timeout,
                execute_request(url, self.hyper_request, telemetry_flow),
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
            },
            None => execute_request(url, self.hyper_request, telemetry_flow).await,
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
    telemetry_flow: Option<TelemetryFlow>,
) -> Result<FlUrlResponse, FlUrlError> {
    if url.is_https {
        return execute_request_https(url, hyper_request, telemetry_flow).await;
    }

    execute_request_http(url, hyper_request, telemetry_flow).await
}

async fn execute_request_https(
    url: FlUrlUriBuilder,
    hyper_request: Request<Body>,
    telemetry_flow: Option<TelemetryFlow>,
) -> Result<FlUrlResponse, FlUrlError> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let result = match client.request(hyper_request).await {
        Ok(response) => Ok(FlUrlResponse::new(url, response)),
        Err(err) => Err(err),
    };

    if let Some(mut telemetry) = telemetry_flow {
        telemetry.write_telemetry(&result).await;
    }

    let result = result?;
    Ok(result)
}

async fn execute_request_http(
    url: FlUrlUriBuilder,
    hyper_request: Request<Body>,
    telemetry_flow: Option<TelemetryFlow>,
) -> Result<FlUrlResponse, FlUrlError> {
    let client = Client::builder().build_http();

    let result = match client.request(hyper_request).await {
        Ok(response) => Ok(FlUrlResponse::new(url, response)),
        Err(err) => Err(err),
    };

    if let Some(mut telemetry) = telemetry_flow {
        telemetry.write_telemetry(&result).await;
    }

    let result = result?;
    Ok(result)
}