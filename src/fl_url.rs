use hyper::Method;

use hyper::Version;
use my_http_client::http1::*;
use my_http_client::MyHttpClientConnector;
use my_tls::tokio_rustls::client::TlsStream;

use rust_extensions::remote_endpoint::Scheme;
use rust_extensions::StrOrString;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

use super::FlUrlResponse;
use crate::http_connectors::*;

use crate::http_clients_cache::*;

use crate::HttpClientResolver;

use crate::FlUrlError;

use crate::FlUrlHeaders;

use url_utils::UrlBuilder;

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
    pub compress_body: bool,
    #[cfg(feature = "with-ssh")]
    ssh_credentials: Option<my_ssh::SshCredentials>,

    max_retries: usize,
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
                    Some(crate::ssh::to_ssh_credentials(&ssh_remote_host)),
                ),
            }
        };

        #[cfg(not(feature = "with-ssh"))]
        let url = {
            let endpoint =
                rust_extensions::remote_endpoint::RemoteEndpointHostString::try_parse(url.as_str())
                    .unwrap();

            match endpoint {
                rust_extensions::remote_endpoint::RemoteEndpointHostString::Direct(
                    _remote_endpoint,
                ) => UrlBuilder::new(url.as_str()),
                rust_extensions::remote_endpoint::RemoteEndpointHostString::ViaSsh {
                    ssh_remote_host: _,
                    remote_host_behind_ssh: _,
                } => panic!("To use ssh you need to enable with-ssh feature"),
            }
        };

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
            request_timeout: Duration::from_secs(10),
            tls_server_name: None,
            compress_body: false,
            #[cfg(feature = "with-ssh")]
            ssh_credentials: credentials,
        }
    }

    #[cfg(feature = "with-ssh")]
    pub fn via_ssh(&self) -> bool {
        self.ssh_credentials.is_some()
    }

    pub fn compress(mut self) -> Self {
        self.compress_body = true;
        self
    }

    pub fn set_not_used_connection_timeout(mut self, timeout: Duration) -> Self {
        self.not_used_connection_timeout = timeout;
        self
    }

    pub fn with_clients_cache(mut self, clients_cache: Arc<HttpClientsCache>) -> Self {
        self.clients_cache = Some(clients_cache);
        self
    }

    pub fn with_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn set_tls_server_name(mut self, domain: String) -> Self {
        self.tls_server_name = Some(domain);
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_password<'s>(mut self, password: impl Into<StrOrString<'s>>) -> Self {
        let ssh_credentials = self.ssh_credentials.take();
        if ssh_credentials.is_none() {
            panic!("To specify ssh password you need to use ssh://user:password@host:port->http://localhost:8080 connection line");
        }
        let ssh_credentials = ssh_credentials.unwrap();

        let (host, port) = ssh_credentials.get_host_port();

        let password = password.into();

        self.ssh_credentials = Some(my_ssh::SshCredentials::UserNameAndPassword {
            ssh_remote_host: host.to_string(),
            ssh_remote_port: port,
            ssh_user_name: ssh_credentials.get_user_name().to_string(),
            password: password.to_string(),
        });
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_private_key<'s>(
        mut self,
        private_key: String,
        passphrase: Option<String>,
    ) -> Self {
        let ssh_credentials = self.ssh_credentials.take();
        if ssh_credentials.is_none() {
            return self;
        }
        let ssh_credentials = ssh_credentials.unwrap();

        let (host, port) = ssh_credentials.get_host_port();

        self.ssh_credentials = Some(my_ssh::SshCredentials::PrivateKey {
            ssh_remote_host: host.to_string(),
            ssh_remote_port: port,
            ssh_user_name: ssh_credentials.get_user_name().to_string(),
            private_key,
            passphrase,
        });
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_user_password<'s>(mut self, password: String) -> Self {
        let ssh_credentials = self.ssh_credentials.take();
        if ssh_credentials.is_none() {
            return self;
        }
        let ssh_credentials = ssh_credentials.unwrap();

        let (host, port) = ssh_credentials.get_host_port();

        self.ssh_credentials = Some(my_ssh::SshCredentials::UserNameAndPassword {
            ssh_remote_host: host.to_string(),
            ssh_remote_port: port,
            ssh_user_name: ssh_credentials.get_user_name().to_string(),
            password,
        });
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

    pub fn with_header<'n, 'v>(
        mut self,
        name: impl Into<StrOrString<'n>>,
        value: impl Into<StrOrString<'v>>,
    ) -> Self {
        let name: StrOrString<'_> = name.into();
        let value: StrOrString<'_> = value.into();

        self.headers.add(name.as_str(), value.as_str());
        self
    }

    pub fn append_raw_ending_to_url<'r>(mut self, raw: impl Into<StrOrString<'r>>) -> Self {
        let raw: StrOrString<'r> = raw.into();
        self.url.append_raw_ending(raw.as_str());
        self
    }

    async fn execute(self, request: MyHttpRequest) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        {
            if self.ssh_credentials.is_some() {
                return self.execute_ssh(request).await;
            }
        }

        let response = match self.url.get_scheme() {
            Scheme::Ws => {
                panic!("WebSocket Ws scheme is not supported")
            }

            Scheme::Wss => {
                panic!("WebSocket Wss scheme is not supported")
            }
            Scheme::Http => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<TcpStream, HttpConnector, _>(
                        &request,
                        &http::HttpClientCreator,
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_clients_cache();
                    self.execute_with_retry::<TcpStream, HttpConnector, _>(
                        &request,
                        clients_cache.as_ref(),
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                }
            }
            Scheme::Https => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector, _>(
                        &request,
                        &https::HttpsClientCreator,
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_clients_cache();

                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector, _>(
                        &request,
                        clients_cache.as_ref(),
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                }
            }
            #[cfg(not(feature = "unix-socket"))]
            Scheme::UnixSocket => {
                panic!("To use unix socket you need to enable unix-socket feature")
            }
            #[cfg(feature = "unix-socket")]
            Scheme::UnixSocket => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<UnixSocketStream, UnixSocketConnector, _>(
                        &request,
                        &unix_socket::UnixSocketHttpClientCreator,
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_clients_cache();

                    self.execute_with_retry::<UnixSocketStream, UnixSocketConnector, _>(
                        &request,
                        clients_cache.as_ref(),
                        #[cfg(feature = "with-ssh")]
                        None,
                    )
                    .await?
                }
            }
        };

        Ok(response)
    }

    #[cfg(feature = "with-ssh")]
    async fn execute_ssh(mut self, request: MyHttpRequest) -> Result<FlUrlResponse, FlUrlError> {
        let ssh_credentials = self.ssh_credentials.take().unwrap();

        let clients_cache = self.get_clients_cache();
        self.execute_with_retry::<my_ssh::SshAsyncChannel, SshHttpConnector, _>(
            &request,
            clients_cache.as_ref(),
            Some(Arc::new(ssh_credentials)),
        )
        .await
    }
    pub(crate) fn get_clients_cache(&self) -> Arc<HttpClientsCache> {
        match self.clients_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::CLIENTS_CACHED.clone(),
        }
    }

    fn compress_body(&mut self, body: Vec<u8>) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};

        if body.len() < 64 {
            return body;
        }

        self.headers.add("Content-Encoding", "gzip");

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_slice()).unwrap();
        let result = encoder.finish().unwrap();

        result
    }

    fn compile_request(&mut self, method: Method, body: Option<Vec<u8>>) -> MyHttpRequest {
        if !self.headers.has_host_header {
            if !self.url.host_is_ip() {
                self.headers
                    .add(hyper::header::HOST.as_str(), self.url.get_host());
            }
        }

        if !self.headers.has_connection_header {
            if !self.do_not_reuse_connection {
                self.headers
                    .add(hyper::header::CONNECTION.as_str(), "keep-alive");
            }
        }

        let mut body = body.unwrap_or_default();

        if self.compress_body {
            body = self.compress_body(body);
        }

        MyHttpRequest::new(
            method,
            self.url.get_path_and_query(),
            Version::HTTP_11,
            self.headers.get_builder(),
            body,
        )
    }

    pub async fn get(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, None);
        self.execute(request).await
    }

    pub async fn head(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, None);
        self.execute(request).await
    }

    pub async fn post(mut self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::POST, body);
        self.execute(request).await
    }

    pub async fn post_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        self.headers.add_json_content_type();
        let request = self.compile_request(Method::POST, body.into());

        self.execute(request).await
    }

    pub async fn patch(mut self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body);
        self.execute(request).await
    }

    pub async fn patch_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        self.headers.add_json_content_type();
        let request = self.compile_request(Method::PATCH, body.into());

        self.execute(request).await
    }

    pub async fn put(mut self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body);
        self.execute(request).await
    }

    pub async fn put_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = serde_json::to_vec(json).unwrap();
        self.headers.add_json_content_type();
        let request = self.compile_request(Method::PUT, body.into());
        self.execute(request).await
    }

    pub async fn delete(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, None);
        self.execute(request).await
    }

    async fn execute_with_retry<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
        THttpClientResolver: HttpClientResolver<TStream, TConnector>,
    >(
        mut self,
        request: &MyHttpRequest,
        http_client_resolver: &THttpClientResolver,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<Arc<my_ssh::SshCredentials>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let mut attempt_no = 0;
        let domain_override = self.tls_server_name.take();
        let client_cert = self.client_cert.take();

        loop {
            let tcp_client = http_client_resolver
                .get_http_client(
                    &self.url,
                    domain_override.as_ref(),
                    client_cert.as_ref(),
                    #[cfg(feature = "with-ssh")]
                    ssh_credentials.as_ref(),
                )
                .await;

            let response = tcp_client.do_request(request, self.request_timeout).await;

            match response {
                Ok(response) => {
                    let response = FlUrlResponse::from_http1_response(self.url, response);
                    return Ok(response);
                }
                Err(err) => {
                    http_client_resolver
                        .drop_http_client(
                            &self.url,
                            #[cfg(feature = "with-ssh")]
                            ssh_credentials.as_ref(),
                        )
                        .await;

                    if attempt_no >= self.max_retries {
                        return Err(FlUrlError::MyHttpClientError(err));
                    }

                    attempt_no += 1;
                }
            }
        }
    }
}
