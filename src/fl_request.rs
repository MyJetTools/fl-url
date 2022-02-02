use hyper::{header::*, Error};
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use std::str::FromStr;

use crate::telemetry_flow::TelemetryFlow;
use crate::FlUrl;

use super::FlUrlResponse;

pub struct FlRequest {
    hyper_request: Request<Body>,
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
        telemetry_flow: Option<TelemetryFlow>,
    ) -> Result<FlUrlResponse, Error> {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);

        let result = match client.request(self.hyper_request).await {
            Ok(response) => Ok(FlUrlResponse::new(response)),
            Err(err) => Err(err),
        };

        if let Some(telemetry) = telemetry_flow {
            telemetry.write_telemetry(&result);
        }

        result
    }
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => Body::from(payload),
        None => Body::empty(),
    }
}
