use hyper::Method;

use rust_extensions::ShortString;
use rust_extensions::StrOrString;

use std::sync::Arc;
use std::time::Duration;

use super::FlUrlResponse;
use crate::DropConnectionScenario;
use crate::HttpClientsCache;

use crate::FlUrlError;

use crate::FlUrlHeaders;
use crate::HttpClient;
use crate::UrlBuilder;

pub struct FlUrl {
    pub url: UrlBuilder,
    pub headers: FlUrlHeaders,
    pub client_cert: Option<my_tls::ClientCertificate>,
    pub accept_invalid_certificate: bool,
    pub execute_timeout: Duration,
    pub do_not_reuse_connection: bool,
    pub clients_cache: Option<Arc<HttpClientsCache>>,
    #[cfg(feature = "with-ssh")]
    ssh_target: crate::ssh::SshTarget,

    pub drop_connection_scenario: Box<dyn DropConnectionScenario + Send + Sync + 'static>,
}

impl FlUrl {
    pub fn new<'s>(url: impl Into<StrOrString<'s>>) -> Self {
        let url: StrOrString<'s> = url.into();
        let url = UrlBuilder::new(ShortString::from_str(url.as_str()).unwrap());

        Self {
            headers: FlUrlHeaders::new(),
            execute_timeout: Duration::from_secs(30),
            client_cert: None,
            url,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            drop_connection_scenario: Box::new(crate::DefaultDropConnectionScenario),
            clients_cache: None,
            #[cfg(feature = "with-ssh")]
            ssh_target: crate::ssh::SshTarget {
                credentials: None,
                session_cache: None,
            },
        }
    }

    pub fn with_clients_cache(mut self, clients_cache: Arc<HttpClientsCache>) -> Self {
        self.clients_cache = Some(clients_cache);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_credentials(mut self, ssh_credentials: Arc<my_ssh::SshCredentials>) -> Self {
        self.ssh_target.credentials = Some(ssh_credentials);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_sessions_cache(
        mut self,
        ssh_credentials: Arc<super::ssh::FlUrlSshSessionsCache>,
    ) -> Self {
        self.ssh_target.session_cache = Some(ssh_credentials);

        self
    }

    pub fn set_timeout(mut self, timeout: Duration) -> Self {
        self.execute_timeout = timeout;
        self
    }
    pub fn set_tls_domain(mut self, domain: impl Into<StrOrString<'static>>) -> Self {
        let domain = domain.into();
        self.url.tls_domain = Some(domain.to_string());
        self
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

    pub fn with_client_certificate(mut self, certificate: my_tls::ClientCertificate) -> Self {
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

    pub fn append_path_segment<'s>(mut self, path_segment: impl Into<StrOrString<'s>>) -> Self {
        self.url.append_path_segment(path_segment.into().as_str());
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

    pub fn with_header<'v>(
        mut self,
        name: impl Into<StrOrString<'static>>,
        value: impl Into<StrOrString<'v>>,
    ) -> Self {
        let name: StrOrString<'static> = name.into();
        let value: StrOrString<'v> = value.into();

        self.headers.add(name, value.to_string());
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
            let scheme_and_host = self.url.get_scheme_and_host();

            let path_and_query = self.url.get_path_and_query();

            let (response, url) = unix_sockets::execute_request(
                scheme_and_host.as_str(),
                path_and_query.as_str(),
                method.as_str(),
                self.headers.iter().map(|itm| (&itm.name, &itm.value)),
                body,
            )
            .await?;

            return Ok(FlUrlResponse::from_unix_response(response, url));
        }

        self.execute_http_or_https(method, body).await
    }

    fn get_clients_cache(&self) -> Arc<HttpClientsCache> {
        match self.clients_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::CLIENTS_CACHED.clone(),
        }
    }

    async fn execute_http_or_https(
        self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        if let Some(ssh_credentials) = &self.ssh_target.credentials {
            let http_client = HttpClient::new(
                &self.url,
                None,
                self.execute_timeout,
                Some(ssh_credentials),
                self.ssh_target.session_cache.as_ref(),
            )
            .await?;

            let result = http_client
                .execute_request(&self.url, method, &self.headers, body, self.execute_timeout)
                .await;

            if result.is_err() {
                if let Some(session_cache) = &self.ssh_target.session_cache {
                    if let Some(ssh_credentials) = &self.ssh_target.credentials {
                        session_cache.remove(ssh_credentials).await;
                    }
                }
            }
            return result;
        }

        let scheme_and_host = self.url.get_scheme_and_host();

        if self.do_not_reuse_connection {
            let client = HttpClient::new(
                &self.url,
                self.client_cert,
                self.execute_timeout,
                #[cfg(feature = "with-ssh")]
                None,
                #[cfg(feature = "with-ssh")]
                None,
            )
            .await?;
            client
                .execute_request(&self.url, method, &self.headers, body, self.execute_timeout)
                .await
        } else {
            let clients_cache = self.get_clients_cache();

            let client = clients_cache
                .get(&self.url, self.execute_timeout, self.client_cert)
                .await?;

            let result = client
                .execute_request(&self.url, method, &self.headers, body, self.execute_timeout)
                .await;

            match result {
                Ok(result) => {
                    if self.drop_connection_scenario.should_we_drop_it(&result) {
                        clients_cache.remove(scheme_and_host.as_str()).await;
                    }
                    return Ok(result);
                }
                Err(err) => {
                    clients_cache.remove(scheme_and_host.as_str()).await;
                    return Err(err);
                }
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

        self.with_header("Content-Type", "application/json")
            .execute(Method::POST, Some(body))
            .await
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::PUT, body).await
    }

    pub async fn put_json(self, json: impl serde::Serialize) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(&json).unwrap();
        self.with_header("Content-Type", "application/json")
            .execute(Method::PUT, Some(body))
            .await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        self.execute(Method::DELETE, None).await
    }
}
