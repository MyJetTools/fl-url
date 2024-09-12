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
    // If we are trying to reuse connection, but it was not used for this time, we will drop it
    pub not_used_connection_timeout: Duration,
    pub do_not_reuse_connection: bool,
    pub clients_cache: Option<Arc<HttpClientsCache>>,
    #[cfg(feature = "with-ssh")]
    ssh_target: crate::ssh::SshTarget,

    pub drop_connection_scenario: Box<dyn DropConnectionScenario + Send + Sync + 'static>,
    max_retries: usize,
    retry_delay: Duration,
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
            not_used_connection_timeout: Duration::from_secs(30),
            max_retries: 0,
            retry_delay: Duration::from_secs(3),
            #[cfg(feature = "with-ssh")]
            ssh_target: crate::ssh::SshTarget {
                credentials: None,
                sessions_pool: None,
                http_buffer_size: 512 * 1024,
            },
        }
    }

    /// Url can be: "http://localhost:8080" or "ssh://user:password@host:port->http://localhost:8080"
    #[cfg(feature = "with-ssh")]
    pub async fn new_with_maybe_ssh<'s>(
        url: impl Into<StrOrString<'s>>,
        ssh_credentials: Option<
            &std::collections::HashMap<String, my_ssh::SshCredentialsSettingsModel>,
        >,
    ) -> Self {
        let url = url.into();
        let over_ssh_config =
            my_ssh::OverSshConnectionSettings::parse(url.as_str(), ssh_credentials).await;

        if over_ssh_config.ssh_credentials.is_none() {
            return Self::new(url);
        }

        Self::new(over_ssh_config.remote_resource_string)
            .set_ssh_credentials(Arc::new(over_ssh_config.ssh_credentials.unwrap()))
    }

    pub fn set_not_used_connection_timeout(mut self, timeout: Duration) -> Self {
        self.not_used_connection_timeout = timeout;
        self
    }

    pub fn with_clients_cache(mut self, clients_cache: Arc<HttpClientsCache>) -> Self {
        self.clients_cache = Some(clients_cache);
        self
    }

    pub fn with_retries(mut self, max_retries: usize, retry_delay: Duration) -> Self {
        self.max_retries = max_retries;
        self.retry_delay = retry_delay;
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_credentials(mut self, ssh_credentials: Arc<my_ssh::SshCredentials>) -> Self {
        self.ssh_target.credentials = Some(ssh_credentials);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_sessions_pool(mut self, ssh_credentials: Arc<my_ssh::SshSessionsPool>) -> Self {
        self.ssh_target.sessions_pool = Some(ssh_credentials);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_http_buffer_size(mut self, buffer_size: usize) -> Self {
        self.ssh_target.http_buffer_size = buffer_size;
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

    async fn execute_json(
        self,
        method: Method,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        self.with_header("Content-Type", "application/json")
            .execute(method, Some(body))
            .await
    }

    async fn execute_json_with_retries(
        self,
        method: Method,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();

        if self.max_retries > 0 {
            return self
                .with_header("Content-Type", "application/json")
                .execute_with_retires(method, Some(body))
                .await;
        }
        self.with_header("Content-Type", "application/json")
            .execute(method, Some(body))
            .await
    }

    async fn execute_with_retires(
        &self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let mut no = 0;
        loop {
            match self.execute(method.clone(), body.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if no >= self.max_retries {
                        return Err(e);
                    }
                    tokio::time::sleep(self.retry_delay).await;
                }
            }

            no += 1;
        }
    }

    async fn execute(
        &self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        {
            if let Some(ssh_credentials) = &self.ssh_target.credentials {
                return self.execute_with_ssh(method, body, ssh_credentials).await;
            }
        }

        let scheme_and_host = self.url.get_scheme_and_host();

        if self.do_not_reuse_connection {
            let client =
                HttpClient::new(&self.url, &self.client_cert, self.execute_timeout).await?;
            return client
                .execute_request(&self.url, method, &self.headers, body, self.execute_timeout)
                .await;
        }

        let clients_cache = self.get_clients_cache();

        let client = clients_cache
            .get_and_reuse(
                &self.url,
                self.execute_timeout,
                &self.client_cert,
                self.not_used_connection_timeout,
            )
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

    #[cfg(feature = "with-ssh")]
    async fn execute_with_ssh(
        &self,
        method: Method,
        body: Option<Vec<u8>>,
        ssh_credentials: &Arc<my_ssh::SshCredentials>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let http_client = HttpClient::new_ssh(
            &self.url,
            self.execute_timeout,
            ssh_credentials,
            self.ssh_target.sessions_pool.as_ref(),
            self.ssh_target.http_buffer_size,
        )
        .await?;

        let result = http_client
            .execute_request(&self.url, method, &self.headers, body, self.execute_timeout)
            .await;

        if result.is_err() {
            println!("Http through ssh failed. Removing session from cache");
            if let Some(session_cache) = &self.ssh_target.sessions_pool {
                if let Some(ssh_credentials) = &self.ssh_target.credentials {
                    session_cache.remove(ssh_credentials).await;
                }
            }
        }
        return result;
    }

    pub(crate) fn get_clients_cache(&self) -> Arc<HttpClientsCache> {
        match self.clients_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::CLIENTS_CACHED.clone(),
        }
    }

    pub async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::GET, None).await;
        }
        self.execute(Method::GET, None).await
    }

    pub async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::HEAD, None).await;
        }
        self.execute(Method::HEAD, None).await
    }

    pub async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::POST, body).await;
        }
        self.execute(Method::POST, body).await
    }

    pub async fn post_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_json_with_retries(Method::POST, json).await;
        }
        self.execute_json(Method::POST, json).await
    }

    pub async fn patch(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::PATCH, body).await;
        }
        self.execute(Method::PATCH, body).await
    }

    pub async fn patch_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_json_with_retries(Method::PATCH, json).await;
        }
        self.execute_json(Method::PATCH, json).await
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::PUT, body).await;
        }
        self.execute(Method::PUT, body).await
    }

    pub async fn put_json(self, json: &impl serde::Serialize) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_json_with_retries(Method::PUT, json).await;
        }
        self.execute_json(Method::PUT, json).await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        if self.max_retries > 0 {
            return self.execute_with_retires(Method::DELETE, None).await;
        }
        self.execute(Method::DELETE, None).await
    }
}
