use hyper::Method;

#[cfg(feature = "with-native-tls")]
use native_tls::Identity;
use rust_extensions::StrOrString;

use std::collections::HashMap;
use std::time::Duration;

use crate::fl_request::FlRequest;

use crate::FlUrlError;
use crate::UrlUriBuilder;

use super::FlUrlResponse;

pub struct FlUrl {
    pub url: UrlUriBuilder,
    pub headers: HashMap<String, String>,
    #[cfg(feature = "with-native-tls")]
    pub client_cert: Option<Identity>,
    #[cfg(feature = "with-native-tls")]
    pub accept_invalid_certificate: bool,
    pub execute_timeout: Option<Duration>,
}

impl FlUrl {
    pub fn new<'s>(url: impl Into<StrOrString<'s>>) -> FlUrl {
        FlUrl {
            url: UrlUriBuilder::new(url),
            headers: HashMap::new(),
            execute_timeout: Some(Duration::from_secs(30)),
            #[cfg(feature = "with-native-tls")]
            client_cert: None,
            #[cfg(feature = "with-native-tls")]
            accept_invalid_certificate: false,
        }
    }

    pub fn new_with_timeout<'s>(url: impl Into<StrOrString<'s>>, time_out: Duration) -> FlUrl {
        FlUrl {
            url: UrlUriBuilder::new(url),
            headers: HashMap::new(),
            execute_timeout: Some(time_out),
            #[cfg(feature = "with-native-tls")]
            client_cert: None,
            #[cfg(feature = "with-native-tls")]
            accept_invalid_certificate: false,
        }
    }

    pub fn new_without_timeout<'s>(url: impl Into<StrOrString<'s>>) -> FlUrl {
        FlUrl {
            url: UrlUriBuilder::new(url),
            headers: HashMap::new(),
            execute_timeout: None,
            #[cfg(feature = "with-native-tls")]
            client_cert: None,
            #[cfg(feature = "with-native-tls")]
            accept_invalid_certificate: false,
        }
    }
    #[cfg(feature = "with-native-tls")]
    pub fn with_client_certificate(mut self, certificate: Identity) -> Self {
        if self.client_cert.is_some() {
            panic!("Client certificate is already set");
        }
        if self.url.get_scheme() != "https" {
            panic!("Client certificate can only be used with https");
        }

        self.client_cert = Some(certificate);
        self
    }
    #[cfg(feature = "with-native-tls")]
    pub fn accept_invalid_certificate(mut self) -> Self {
        self.accept_invalid_certificate = true;
        self
    }

    pub fn append_path_segment<'s>(mut self, path_segment: impl Into<StrOrString<'s>>) -> Self {
        let path_segment: StrOrString<'s> = path_segment.into();
        self.url.append_path_segment(path_segment.to_string());
        self
    }

    pub fn append_query_param<'n, 'v>(
        mut self,
        param_name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> Self {
        let param_name = param_name.into().to_string();

        let value: Option<String> = if let Some(value) = value {
            Some(value.into().to_string())
        } else {
            None
        };

        self.url.append_query_param(param_name, value);
        self
    }

    pub fn with_header<'n, 'v>(
        mut self,
        name: impl Into<StrOrString<'n>>,
        value: impl Into<StrOrString<'v>>,
    ) -> Self {
        let name: StrOrString<'n> = name.into();
        let value: StrOrString<'v> = value.into();

        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    pub fn append_raw_ending_to_url<'r>(mut self, raw: impl Into<StrOrString<'r>>) -> Self {
        let raw: StrOrString<'r> = raw.into();
        self.url.append_raw_ending(raw.to_string());
        self
    }

    async fn execute(
        self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = FlRequest::new(self, method, body);

        request.execute().await
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
