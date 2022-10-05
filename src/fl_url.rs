use hyper::Method;
#[cfg(feature = "with-client-cert")]
use native_tls::Identity;

use std::collections::HashMap;
use std::time::Duration;

use crate::fl_request::FlRequest;

use crate::FlUrlError;
use crate::FlUrlUriBuilder;

use super::FlUrlResponse;
#[cfg(feature = "with-client-cert")]
pub struct CertInfo {
    pub certificate: Identity,
    pub accept_invalid_cert: bool,
}

pub struct FlUrl {
    pub url: FlUrlUriBuilder,
    pub headers: HashMap<String, String>,
    #[cfg(feature = "with-client-cert")]
    pub client_cert: Option<CertInfo>,
    execute_timeout: Option<Duration>,
}

impl FlUrl {
    pub fn new(url: &str) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            execute_timeout: Some(Duration::from_secs(30)),
            #[cfg(feature = "with-client-cert")]
            client_cert: None,
        }
    }

    pub fn new_with_timeout(url: &str, time_out: Duration) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            execute_timeout: Some(time_out),
            #[cfg(feature = "with-client-cert")]
            client_cert: None,
        }
    }

    pub fn new_without_timeout(url: &str) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),
            headers: HashMap::new(),
            execute_timeout: None,
            #[cfg(feature = "with-client-cert")]
            client_cert: None,
        }
    }

    #[cfg(feature = "with-client-cert")]
    pub fn with_client_certificate(
        mut self,
        certificate: Identity,
        accept_invalid_cert: bool,
    ) -> Self {
        if self.client_cert.is_some() {
            panic!("Client certificate is already set");
        }
        if self.url.get_scheme() != "https" {
            panic!("Client certificate can only be used with https");
        }

        self.client_cert = Some(CertInfo {
            accept_invalid_cert,
            certificate,
        });
        self
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

    pub fn append_raw_ending(mut self, raw: &str) -> Self {
        self.url.append_raw_ending(raw);
        self
    }

    async fn execute(
        mut self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = FlRequest::new(&mut self, method, body);
        let execute_timeout = self.execute_timeout;

        request
            .execute(
                self.url,
                execute_timeout,
                #[cfg(feature = "with-client-cert")]
                self.client_cert.take(),
            )
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
