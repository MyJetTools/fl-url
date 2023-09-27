use hyper::Method;

use rust_extensions::StrOrString;

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use super::FlUrlResponse;
use crate::ClientsCache;
use crate::DropConnectionScenario;
use crate::FlUrlClient;
use crate::FlUrlError;
use crate::FlUrlFactory;
use crate::UrlBuilder;

lazy_static::lazy_static! {
    static ref CLIENTS_CACHED: ClientsCache = ClientsCache::new();
}

pub struct FlUrl {
    pub url: UrlBuilder,
    pub headers: HashMap<String, String>,
    pub client_cert: Option<crate::ClientCertificate>,
    pub accept_invalid_certificate: bool,
    pub execute_timeout: Option<Duration>,
    pub do_not_reuse_connection: bool,

    pub drop_connection_scenario: Box<dyn DropConnectionScenario + Send + Sync + 'static>,
}

impl FlUrl {
    pub fn new(url: impl Into<StrOrString<'static>>) -> Self {
        let url = UrlBuilder::new(url);
        Self {
            headers: HashMap::new(),
            execute_timeout: Some(Duration::from_secs(30)),
            client_cert: None,
            url,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            drop_connection_scenario: Box::new(crate::DefaultDropConnectionScenario),
        }
    }

    pub fn new_with_timeout(url: impl Into<StrOrString<'static>>, time_out: Duration) -> Self {
        let url = UrlBuilder::new(url);
        Self {
            headers: HashMap::new(),
            execute_timeout: Some(time_out),
            url,
            client_cert: None,
            do_not_reuse_connection: false,
            accept_invalid_certificate: false,
            drop_connection_scenario: Box::new(crate::DefaultDropConnectionScenario),
        }
    }

    pub fn new_without_timeout(url: impl Into<StrOrString<'static>>) -> Self {
        Self {
            url: UrlBuilder::new(url),
            headers: HashMap::new(),
            execute_timeout: None,
            client_cert: None,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            drop_connection_scenario: Box::new(crate::DefaultDropConnectionScenario),
        }
    }

    pub fn override_drop_connection_scenario(
        mut self,
        drop_connection_scenario: impl DropConnectionScenario + Send + Sync + 'static,
    ) -> Self {
        self.drop_connection_scenario = Box::new(drop_connection_scenario);
        self
    }

    pub fn do_not_reuse_connection(mut self) -> Self {
        self.do_not_reuse_connection = true;
        self
    }

    pub fn with_client_certificate(
        mut self,
        certificate: crate::client_certificate::ClientCertificate,
    ) -> Self {
        if self.client_cert.is_some() {
            panic!("Client certificate is already set");
        }
        if !self.url.get_scheme().is_https() {
            panic!("Client certificate can only be used with https");
        }

        self.client_cert = Some(certificate);
        self
    }

    pub fn accept_invalid_certificate(mut self) -> Self {
        self.accept_invalid_certificate = true;
        self
    }

    pub fn append_path_segment(mut self, path_segment: impl Into<StrOrString<'static>>) -> Self {
        self.url.append_path_segment(path_segment.into());
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
        #[cfg(feature = "support-unix-socket")]
        if self.url.scheme.is_unix_socket() {
            return self.execute_unix_socket().await;
        }

        self.execute_http_or_https(method, body).await
    }

    async fn execute_http_or_https(
        mut self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let url = self.url.to_string();

        let mut req = hyper::Request::builder().method(method).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in &self.headers {
                let header_name = hyper::http::HeaderName::from_str(key).unwrap();
                headers.insert(
                    header_name,
                    hyper::http::HeaderValue::from_str(value).unwrap(),
                );
            }
        };

        let body = req.body(hyper::Body::from(compile_body(body))).unwrap();

        let scheme_and_host = self.url.get_scheme_and_host().as_str().to_lowercase();

        let result = if self.do_not_reuse_connection {
            let client = self.create();
            client.execute(self.url, body).await
        } else {
            let client = CLIENTS_CACHED
                .get(scheme_and_host.as_str(), &mut self)
                .await;

            client.execute(self.url, body).await
        };

        match result {
            Ok(result) => {
                if self.drop_connection_scenario.should_we_drop_it(&result) {
                    CLIENTS_CACHED.remove(scheme_and_host.as_str()).await;
                }
                return Ok(result);
            }
            Err(err) => {
                CLIENTS_CACHED.remove(scheme_and_host.as_str()).await;
                return Err(err);
            }
        }
    }
    #[cfg(feature = "support-unix-socket")]
    async fn execute_unix_socket(self) -> Result<FlUrlResponse, FlUrlError> {
        use hyper_unix_connector::UnixClient;
        let client: hyper::Client<UnixClient, hyper::Body> =
            hyper::Client::builder().build(UnixClient);
        let addr: hyper::Uri = hyper_unix_connector::Uri::new(
            self.url.get_scheme_and_host().as_str(),
            &self.url.get_path_and_query(),
        )
        .into();

        let result = client.get(addr).await;

        match result {
            Ok(result) => {
                return Ok(FlUrlResponse::new(self.url, result));
            }
            Err(err) => {
                return Err(FlUrlError::HyperError(err));
            }
        }
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

    pub async fn post_json(self, json: impl serde::Serialize) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(&json).unwrap();
        self.execute(Method::POST, Some(body)).await
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::PUT, body).await
    }

    pub async fn put_json(self, json: impl serde::Serialize) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(&json).unwrap();
        self.execute(Method::PUT, Some(body)).await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::DELETE, None).await
    }
}

impl FlUrlFactory for FlUrl {
    fn create(&mut self) -> FlUrlClient {
        match self.url.scheme {
            crate::Scheme::Http => FlUrlClient::new_http(),
            crate::Scheme::Https => FlUrlClient::new_https(self.client_cert.take()),
            #[cfg(feature = "support-unix-socket")]
            crate::Scheme::UnixSocket => panic!("Unix socket is not supported in this case"),
        }
    }
}

fn compile_body(body_payload: Option<Vec<u8>>) -> hyper::body::Body {
    match body_payload {
        Some(payload) => hyper::Body::from(payload),
        None => hyper::Body::empty(),
    }
}
