use bytes::Bytes;
use http_body_util::Full;

use hyper::Method;

use my_http_client::http1::MyHttpClient;
use rust_extensions::StrOrString;

use std::sync::Arc;
use std::time::Duration;

use super::FlUrlResponse;
use crate::HttpClientsCache;

use crate::FlUrlError;

use crate::FlUrlHeaders;
use crate::UrlBuilder;

pub struct FlUrl {
    pub url: UrlBuilder,
    pub headers: FlUrlHeaders,
    pub client_cert: Option<my_tls::ClientCertificate>,
    pub accept_invalid_certificate: bool,
    pub execute_timeout: Duration,
    // If we are trying to reuse connection, but it was not used for this time, we will drop it
    pub not_used_connection_timeout: Duration,
    pub request_timeout: Duration,
    pub do_not_reuse_connection: bool,
    pub clients_cache: Option<Arc<HttpClientsCache>>,
    pub tls_server_name: Option<String>,
    #[cfg(feature = "with-ssh")]
    ssh_target: crate::ssh::SshTarget,

    max_retries: usize,
    retry_delay: Duration,
}

impl FlUrl {
    pub fn new<'s>(url: impl Into<StrOrString<'s>>) -> Self {
        let url: StrOrString<'s> = url.into();

        #[cfg(feature = "with-ssh")]
        let (url, credentials) = {
            let endpoint =
                rust_extensions::remote_endpoint::RemoteEndpointHostString::try_parse(url.as_str())
                    .unwrap();

            match endpoint {
                rust_extensions::remote_endpoint::RemoteEndpointHostString::Direct(
                    _remote_endpoint,
                ) => (UrlBuilder::new(url.as_str()), None),
                rust_extensions::remote_endpoint::RemoteEndpointHostString::ViaSsh {
                    ssh_remote_host,
                    remote_host_behind_ssh,
                } => (
                    UrlBuilder::new(remote_host_behind_ssh.as_str()),
                    Some(Arc::new(crate::ssh::to_ssh_credentials(&ssh_remote_host))),
                ),
            }
        };

        #[cfg(not(feature = "with-ssh"))]
        let url = UrlBuilder::new(url.as_str());

        Self {
            headers: FlUrlHeaders::new(),
            execute_timeout: Duration::from_secs(30),
            client_cert: None,
            url,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            clients_cache: None,
            not_used_connection_timeout: Duration::from_secs(30),
            max_retries: 0,
            retry_delay: Duration::from_secs(3),
            request_timeout: Duration::from_secs(10),
            tls_server_name: None,
            #[cfg(feature = "with-ssh")]
            ssh_target: crate::ssh::SshTarget {
                credentials,
                sessions_pool: None,
                http_buffer_size: 512 * 1024,
            },
        }
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

    pub fn set_tls_server_name(mut self, domain: String) -> Self {
        self.tls_server_name = Some(domain);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_password<'s>(mut self, password: impl Into<StrOrString<'s>>) -> Self {
        let ssh_credentials = self.ssh_target.credentials.take();
        if ssh_credentials.is_none() {
            panic!("To specify ssh password you need to use ssh://user:password@host:port->http://localhost:8080 connection line");
        }
        let ssh_credentials = ssh_credentials.unwrap();

        let (host, port) = ssh_credentials.get_host_port();

        let password = password.into();

        self.ssh_target.credentials = Some(Arc::new(my_ssh::SshCredentials::UserNameAndPassword {
            ssh_remote_host: host.to_string(),
            ssh_remote_port: port,
            ssh_user_name: ssh_credentials.get_user_name().to_string(),
            password: password.to_string(),
        }));
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_private_key<'s>(
        mut self,
        private_key: String,
        passphrase: Option<String>,
    ) -> Self {
        let ssh_credentials = self.ssh_target.credentials.take();
        if ssh_credentials.is_none() {
            panic!("To specify ssh password you need to use ssh://user:password@host:port->http://localhost:8080 connection line");
        }
        let ssh_credentials = ssh_credentials.unwrap();

        let (host, port) = ssh_credentials.get_host_port();

        self.ssh_target.credentials = Some(Arc::new(my_ssh::SshCredentials::PrivateKey {
            ssh_remote_host: host.to_string(),
            ssh_remote_port: port,
            ssh_user_name: ssh_credentials.get_user_name().to_string(),
            private_key,
            passphrase,
        }));
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
        let param_name = param_name.into();

        if let Some(value) = value {
            let value = value.into();
            self.url
                .append_query_param(param_name.as_str(), Some(value.as_str()));
        } else {
            self.url.append_query_param(param_name.as_str(), None);
        };

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
        self.url.append_raw_ending(raw.as_str());
        self
    }

    async fn execute(
        mut self,
        request: hyper::Request<Full<Bytes>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        {
            if let Some(ssh_credentials) = self.ssh_target.credentials.clone() {
                return self.execute_with_ssh(request, ssh_credentials).await;
            }
        }

        let response = match self.url.get_scheme() {
            crate::Scheme::Http => {
                if self.do_not_reuse_connection {
                    let remote_endpoint = self.url.get_remote_endpoint();
                    let http_connector =
                        crate::http_connectors::HttpConnector::new(remote_endpoint.to_owned());
                    let client = MyHttpClient::new(http_connector);
                    let response = client.do_request(request, self.request_timeout).await?;
                    FlUrlResponse::from_http1_response(self.url, response)
                } else {
                    let reused_connection = self
                        .get_clients_cache()
                        .get_http_and_reuse(&self.url)
                        .await?;

                    let response = reused_connection
                        .do_request(request, self.request_timeout)
                        .await?;
                    FlUrlResponse::from_http1_response(self.url, response)
                }
            }
            crate::Scheme::Https => {
                if self.do_not_reuse_connection {
                    let http_connector = crate::http_connectors::HttpsConnector::new(
                        self.url.get_remote_endpoint().to_owned(),
                        self.tls_server_name.take(),
                        self.client_cert.take(),
                    );
                    let client = MyHttpClient::new(http_connector);
                    let response = client.do_request(request, self.request_timeout).await?;
                    FlUrlResponse::from_http1_response(self.url, response)
                } else {
                    let reused_connection = self
                        .get_clients_cache()
                        .get_https_and_reuse(
                            &self.url,
                            self.tls_server_name.take(),
                            self.client_cert.take(),
                        )
                        .await?;

                    let response = reused_connection
                        .do_request(request, self.request_timeout)
                        .await?;

                    FlUrlResponse::from_http1_response(self.url, response)
                }
            }
            #[cfg(not(feature = "unix-socket"))]
            crate::Scheme::UnixSocket => {
                panic!("To use unix socket you need to enable unix-socket feature")
            }

            #[cfg(feature = "unix-socket")]
            crate::Scheme::UnixSocket => {
                if self.do_not_reuse_connection {
                    let remote_endpoint = self.url.get_remote_endpoint();
                    let http_connector = crate::http_connectors::UnixSocketConnector::new(
                        remote_endpoint.to_owned(),
                    );
                    let client = MyHttpClient::new(http_connector);
                    let response = client.do_request(request, self.request_timeout).await?;
                    FlUrlResponse::from_http1_response(self.url, response)
                } else {
                    let reused_connection = self
                        .get_clients_cache()
                        .get_unix_socket_and_reuse(&self.url)
                        .await?;

                    let response = reused_connection
                        .do_request(request, self.request_timeout)
                        .await?;

                    FlUrlResponse::from_http1_response(self.url, response)
                }
            }
        };

        Ok(response)
    }

    #[cfg(feature = "with-ssh")]
    async fn execute_with_ssh(
        self,
        request: hyper::Request<Full<Bytes>>,
        ssh_credentials: Arc<my_ssh::SshCredentials>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let reused_connection = self
            .get_clients_cache()
            .get_ssh_and_reuse(&self.url, &ssh_credentials)
            .await?;

        let response = reused_connection
            .do_request(request, self.request_timeout)
            .await?;

        let result = FlUrlResponse::from_http1_response(self.url, response);

        Ok(result)
    }

    pub(crate) fn get_clients_cache(&self) -> Arc<HttpClientsCache> {
        match self.clients_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::CLIENTS_CACHED.clone(),
        }
    }

    fn compile_request(
        &self,
        method: Method,
        body: Option<Vec<u8>>,
    ) -> hyper::Request<Full<Bytes>> {
        let mut request = hyper::Request::builder()
            .method(method)
            .uri(self.url.to_string());

        for header in self.headers.iter() {
            request = request.header(header.name.as_str(), header.value.as_str());
        }

        match body {
            Some(body) => request.body(Full::from(Bytes::from(body))).unwrap(),
            None => {
                let body = Bytes::from(vec![]);
                request.body(Full::from(body)).unwrap()
            }
        }
    }

    pub async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, None);
        self.execute(request).await
    }

    pub async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, None);
        self.execute(request).await
    }

    pub async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, body);
        self.execute(request).await
    }

    pub async fn post_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        let fl_url = self.with_header("Content-Type", "application/json");
        let request = fl_url.compile_request(Method::POST, Some(body));

        fl_url.execute(request).await
    }

    pub async fn patch(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body);
        self.execute(request).await
    }

    pub async fn patch_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        let fl_url = self.with_header("Content-Type", "application/json");
        let request = fl_url.compile_request(Method::PATCH, Some(body));

        fl_url.execute(request).await
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body);
        self.execute(request).await
    }

    pub async fn put_json(self, json: &impl serde::Serialize) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        let fl_url = self.with_header("Content-Type", "application/json");
        let request = fl_url.compile_request(Method::PUT, Some(body));
        fl_url.execute(request).await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, None);
        self.execute(request).await
    }
}
