use hyper::Method;
use my_telemetry::MyTelemetryContext;
use std::collections::HashMap;
use std::time::Duration;

use crate::fl_request::FlRequest;
use crate::telemetry_flow::TelemetryData;
use crate::telemetry_flow::TelemetryFlow;
use crate::FlUrlError;
use crate::FlUrlUriBuilder;

use super::FlUrlResponse;

pub struct FlUrl {
    pub url: FlUrlUriBuilder,
    pub headers: HashMap<String, String>,
    pub telemetry_flow: Option<TelemetryFlow>,
    execute_timeout: Option<Duration>,
}

impl FlUrl {
    pub fn new(url: &str, telemetry_context: Option<MyTelemetryContext>) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry_flow: TelemetryFlow::new(telemetry_context),
            execute_timeout: Some(Duration::from_secs(30)),
        }
    }

    pub fn new_with_timeout(
        url: &str,
        time_out: Duration,
        telemetry_context: Option<MyTelemetryContext>,
    ) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry_flow: TelemetryFlow::new(telemetry_context),
            execute_timeout: Some(time_out),
        }
    }

    pub fn new_without_timeout(url: &str, telemetry_context: Option<MyTelemetryContext>) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry_flow: TelemetryFlow::new(telemetry_context),
            execute_timeout: None,
        }
    }

    pub fn append_path_segment(mut self, path: &str) -> Self {
        self.url.append_path_segment(path);
        self
    }

    pub fn append_query_param(mut self, param: &str, value: &str) -> Self {
        self.url.append_query_param(param, Some(value.to_string()));
        self
    }

    pub fn set_query_param(mut self, param: &str) -> Self {
        self.url.append_query_param(param, None);
        self
    }

    pub fn append_query_param_string(mut self, param: &str, value: String) -> Self {
        self.url.append_query_param(param, Some(value));
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    pub fn with_header_val_string(mut self, name: &str, value: String) -> Self {
        self.headers.insert(name.to_string(), value);
        self
    }

    async fn execute(
        mut self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = FlRequest::new(&self, method, body);
        let execute_timeout = self.execute_timeout;

        if let Some(telemetry) = &mut self.telemetry_flow {
            telemetry.data = Some(TelemetryData {
                method: request.hyper_request.method().clone(),
                url: self.url.to_string(),
            });
        }

        request
            .execute(self.url.is_https, execute_timeout, self.telemetry_flow)
            .await
    }

    pub async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::GET, None).await
    }

    pub async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::HEAD, None).await
    }

    pub async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::POST, body).await
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::PUT, body).await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::DELETE, None).await
    }
}
