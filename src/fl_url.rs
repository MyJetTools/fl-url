use hyper::Method;
use my_telemetry::MyTelemetry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::fl_request::FlRequest;
use crate::stop_watch::StopWatch;
use crate::telemetry_flow::TelemetryFlow;
use crate::FlUrlError;
use crate::FlUrlUriBuilder;

use super::FlUrlResponse;

pub struct FlUrlTelemetry {
    pub telemetry: Arc<dyn MyTelemetry + Send + Sync + 'static>,
    pub dependency_type: String,
}

pub struct FlUrl {
    pub url: FlUrlUriBuilder,
    pub headers: HashMap<String, String>,
    pub telemetry: Option<FlUrlTelemetry>,
    execute_timeout: Option<Duration>,
}

impl<'t> FlUrl {
    pub fn new(url: &'t str) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry: None,
            execute_timeout: Some(Duration::from_secs(30)),
        }
    }

    pub fn new_with_timeout(url: &'t str, time_out: Duration) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry: None,
            execute_timeout: Some(time_out),
        }
    }

    pub fn new_without_timeout(url: &'t str) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            telemetry: None,
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

    fn get_telemetry(&self, verb: &str) -> Option<TelemetryFlow> {
        let telemetry = self.telemetry.as_ref()?;

        let mut sw = StopWatch::new();
        sw.start();

        TelemetryFlow {
            telemetry: telemetry.telemetry.clone(),
            sw,
            target: self.url.get_host().to_string(),
            dependency_type: telemetry.dependency_type.to_string(),
            name: format!("{} {}", verb, self.url.get_path()),
        }
        .into()
    }

    async fn execute(
        self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let telemetry = self.get_telemetry(method.as_str());
        let request = FlRequest::new(&self, method, body);
        let execute_timeout = self.execute_timeout;
        request
            .execute(self.url.is_https, execute_timeout, telemetry)
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
