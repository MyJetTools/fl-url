use bytes::Bytes;
use http_body_util::Full;
use hyper::Method;

use hyper::Uri;
use hyper::Version;
use my_http_client::http1::MyHttpRequestBuilder;
use my_http_client::MyHttpClientConnector;
use my_tls::tokio_rustls::client::TlsStream;

use rust_extensions::remote_endpoint::Scheme;
use rust_extensions::StrOrString;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

use super::FlUrlResponse;
use crate::body::HttpRequestBody;
use crate::compiled_http_request::CompiledHttpRequest;
use crate::http_connectors::*;

use crate::http_clients_cache::*;

use crate::HttpConnectionResolver;

use crate::FlUrlError;

use crate::FlUrlHeaders;

use url_utils::UrlBuilder;

#[derive(Debug, Clone, Copy)]
pub enum FlUrlMode {
    H2,
    Http1NoHyper,
    Http1Hyper,
}

impl FlUrlMode {
    pub fn is_h2(&self) -> bool {
        match self {
            Self::H2 => true,
            _ => false,
        }
    }
}

impl Default for FlUrlMode {
    fn default() -> Self {
        Self::Http1Hyper
    }
}

/// HTTP verb selector for [`FlUrl::execute_request`].
#[derive(Clone, Copy, Debug)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

pub struct FlUrl {
    pub url_builder: UrlBuilder,
    pub headers: FlUrlHeaders,
    pub client_cert: Option<my_tls::ClientCertificate>,
    pub accept_invalid_certificate: bool,
    // If we are trying to reuse connection, but it was not used for this time, we will drop it
    pub not_used_connection_timeout: Duration,
    pub request_timeout: Duration,
    // Bounds how long reading the response body may take. `None` = unbounded.
    pub response_body_timeout: Option<Duration>,
    pub do_not_reuse_connection: bool,
    pub connections_cache: Option<Arc<FlUrlHttpConnectionsCache>>,
    pub compress_body: bool,
    pub decompress_gzip_response: bool,
    pub print_input_request: bool,
    // If we reuse connection and it has not been used more seconds than this parameter - it disposed
    pub reuse_connection_timeout_sec: i64,
    mode: FlUrlMode,
    #[cfg(all(unix, feature = "with-ssh"))]
    ssh_credentials: Option<my_ssh::SshCredentials>,
    #[cfg(all(unix, feature = "with-ssh"))]
    ssh_security_credentials_resolver:
        Option<Arc<dyn my_ssh::ssh_settings::SshSecurityCredentialsResolver + Send + Sync>>,

    max_retries: usize,
}

impl FlUrl {
    pub fn new<'s>(url: impl Into<StrOrString<'s>>) -> Self {
        return Self::try_new(url).unwrap();
    }

    pub fn try_new<'s>(url: impl Into<StrOrString<'s>>) -> Result<Self, FlUrlError> {
        let url: StrOrString<'s> = url.into();

        #[cfg(all(unix, feature = "with-ssh"))]
        let (url, credentials) = {
            let endpoint =
                rust_extensions::remote_endpoint::RemoteEndpointHostString::try_parse(url.as_str())
                    .map_err(|err| FlUrlError::InvalidUrl(err))?;

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

        #[cfg(not(all(unix, feature = "with-ssh")))]
        let url = {
            let endpoint =
                rust_extensions::remote_endpoint::RemoteEndpointHostString::try_parse(url.as_str())
                    .map_err(|err| FlUrlError::InvalidUrl(err))?;

            match endpoint {
                rust_extensions::remote_endpoint::RemoteEndpointHostString::Direct(
                    _remote_endpoint,
                ) => UrlBuilder::new(url.as_str()),
                rust_extensions::remote_endpoint::RemoteEndpointHostString::ViaSsh {
                    ssh_remote_host: _,
                    remote_host_behind_ssh: _,
                } => {
                    return Err(FlUrlError::UnsupportedScheme(
                        "To use ssh you need to enable the 'with-ssh' feature".to_string(),
                    ))
                }
            }
        };

        let result = Self {
            headers: FlUrlHeaders::new(),
            client_cert: Default::default(),
            url_builder: url,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            connections_cache: Default::default(),
            not_used_connection_timeout: Duration::from_secs(30),
            max_retries: 0,
            request_timeout: Duration::from_secs(10),
            response_body_timeout: None,
            print_input_request: false,
            compress_body: false,
            decompress_gzip_response: false,
            #[cfg(all(unix, feature = "with-ssh"))]
            ssh_credentials: credentials,
            #[cfg(all(unix, feature = "with-ssh"))]
            ssh_security_credentials_resolver: None,
            mode: Default::default(),
            reuse_connection_timeout_sec: 120,
        };

        Ok(result)
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    pub fn via_ssh(&self) -> bool {
        self.ssh_credentials.is_some()
    }

    pub fn compress(mut self) -> Self {
        self.compress_body = true;
        self
    }

    /// Advertises gzip support to the server (`Accept-Encoding: gzip`) and
    /// transparently decompresses a gzip-encoded response body on buffered
    /// reads (`get_body_as_slice`, `get_json`, `get_body_as_str`, `receive_body`).
    /// Streamed bodies (`get_body_as_stream`) are NOT decompressed.
    pub fn accept_gzip(mut self) -> Self {
        if !self.headers.has_header("Accept-Encoding") {
            self.headers.add("Accept-Encoding", "gzip");
        }
        self.decompress_gzip_response = true;
        self
    }

    pub fn set_not_used_connection_timeout(mut self, timeout: Duration) -> Self {
        self.not_used_connection_timeout = timeout;
        // Round up and clamp to at least 1s: as_secs() truncation would turn a
        // sub-second timeout into 0, which evicts the whole per-key pool on
        // every checkout (pooling silently disabled).
        self.reuse_connection_timeout_sec = (timeout.as_secs_f64().ceil() as i64).max(1);
        self
    }

    pub fn update_mode(mut self, mode: FlUrlMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn set_connections_cache(mut self, clients_cache: Arc<FlUrlHttpConnectionsCache>) -> Self {
        self.connections_cache = Some(clients_cache);
        self
    }

    /// Retries the request up to `max_retries` extra times on failure. Only
    /// IDEMPOTENT methods are replayed (a POST that may have reached the server
    /// is never re-sent). Note that my-http-client performs its own internal
    /// reconnect/retry cycles per attempt, so each outer retry is a full fresh
    /// cycle on top of those — keep this number small.
    pub fn with_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn print_input_request(mut self) -> Self {
        self.print_input_request = true;
        self
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    pub fn set_ssh_security_credentials_resolver(
        mut self,
        resolver: Arc<dyn my_ssh::ssh_settings::SshSecurityCredentialsResolver + Send + Sync>,
    ) -> Self {
        self.ssh_security_credentials_resolver = Some(resolver);
        self
    }

    #[cfg(all(unix, feature = "with-ssh"))]
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

    #[cfg(all(unix, feature = "with-ssh"))]
    pub fn set_ssh_credentials(mut self, ssh_credentials: my_ssh::SshCredentials) -> Self {
        self.ssh_credentials = Some(ssh_credentials);
        self
    }

    #[cfg(all(unix, feature = "with-ssh"))]
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

    #[cfg(all(unix, feature = "with-ssh"))]
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
        self.request_timeout = timeout;
        self
    }

    /// Bounds how long reading the response body may take. Applies both to
    /// buffered reads (`get_body_as_slice`, `get_json`, …) and to each chunk of
    /// a streamed body. Unbounded by default.
    pub fn set_response_body_timeout(mut self, timeout: Duration) -> Self {
        self.response_body_timeout = Some(timeout);
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
        if !self.url_builder.get_scheme().is_https() {
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
        self.url_builder
            .append_path_segment(path_segment.into().as_str());
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
            self.url_builder
                .append_query_param(param_name.as_str(), Some(value.as_str()));
        } else {
            self.url_builder
                .append_query_param(param_name.as_str(), None);
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
        self.url_builder.append_raw_ending(raw.as_str());
        self
    }

    /// Pours a `url_utils` request model into this `FlUrl`: the model appends its
    /// path segments + query params to our `url_builder`, pushes its header fields
    /// into our `headers`, and hands over its body (which it consumes). The base
    /// host and any static route prefix must already be configured on `self`.
    fn fill_from_model(
        &mut self,
        model: impl url_utils::schema::client::THttpRequestBuilder,
    ) -> Result<HttpRequestBody, FlUrlError> {
        model.fill_url(&mut self.url_builder)?;
        model.fill_headers(&mut self.headers)?;
        // `get_body` consumes the model, so it must be the last thing we read.
        // The body is our own `HttpRequestBody` already — no conversion needed;
        // `compile_*_request` reads its (possibly dynamic, e.g. FormData boundary)
        // content type via `get_content_type()`. `FlUrlRnd` supplies the random
        // multipart boundary suffix (url-utils carries no RNG of its own).
        let body = model.get_body::<crate::body::FlUrlRnd>()?;
        Ok(body)
    }

    /// Executes an HTTP request described by a `url_utils` request model (any type
    /// deriving `url_utils::macros::MyHttpInput`). The model fills the URL
    /// path/query, headers, and body; `verb` selects the method. The base host and
    /// any static route prefix are configured on `self` beforehand via the usual
    /// builder methods (`append_path_segment`, `with_header`, …).
    ///
    /// `Get`/`Delete`/`Head` do not carry a body, so a body produced by the model
    /// is ignored for those verbs.
    pub async fn execute_request(
        mut self,
        verb: HttpVerb,
        model: impl url_utils::schema::client::THttpRequestBuilder,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = self.fill_from_model(model)?;

        match verb {
            HttpVerb::Get => self.get().await,
            HttpVerb::Delete => self.delete().await,
            HttpVerb::Head => self.head().await,
            HttpVerb::Post => self.post(body).await,
            HttpVerb::Put => self.put(body).await,
            HttpVerb::Patch => self.patch(body).await,
        }
    }

    async fn execute(self, request: CompiledHttpRequest) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(all(unix, feature = "with-ssh"))]
        if self.ssh_credentials.is_some() {
            let mut self_mut = self;
            let ssh_credentials = self_mut.ssh_credentials.take().unwrap();
            return self_mut.execute_ssh(request, ssh_credentials).await;
        }

        let response = match self.url_builder.get_scheme() {
            Scheme::Ws => {
                return Err(FlUrlError::UnsupportedScheme(
                    "WebSocket 'ws' scheme is not supported".to_string(),
                ))
            }

            Scheme::Wss => {
                return Err(FlUrlError::UnsupportedScheme(
                    "WebSocket 'wss' scheme is not supported".to_string(),
                ))
            }
            Scheme::Http => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<TcpStream, HttpConnector>(
                        &request,
                        Arc::new(crate::http_clients_cache::creators::HttpConnectionCreator),
                        crate::consts::HTTP_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();
                    self.execute_with_retry::<TcpStream, HttpConnector>(
                        &request,
                        clients_cache,
                        crate::consts::HTTP_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                }
            }
            Scheme::Https => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector>(
                        &request,
                        Arc::new(crate::http_clients_cache::creators::HttpsConnectionCreator),
                        crate::consts::HTTPS_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();

                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector>(
                        &request,
                        clients_cache,
                        crate::consts::HTTPS_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                }
            }
            #[cfg(not(unix))]
            Scheme::UnixSocket => {
                return Err(FlUrlError::UnsupportedScheme(
                    "This OS does not support unix sockets".to_string(),
                ))
            }
            #[cfg(unix)]
            Scheme::UnixSocket => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<UnixSocketStream, UnixSocketConnector>(
                        &request,
                        Arc::new(crate::http_clients_cache::creators::UnixSocketHttpClientCreator),
                        None,
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();

                    self.execute_with_retry::<UnixSocketStream, UnixSocketConnector>(
                        &request,
                        clients_cache,
                        None,
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                }
            }
        };

        Ok(response)
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    async fn execute_ssh(
        mut self,
        request: CompiledHttpRequest,
        mut ssh_credentials: my_ssh::SshCredentials,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if let Some(private_key_resolver) = self.ssh_security_credentials_resolver.take() {
            ssh_credentials = private_key_resolver
                .update_credentials(&ssh_credentials)
                .await;
        }

        if self.do_not_reuse_connection {
            return self
                .execute_with_retry::<my_ssh::SshAsyncChannel, SshHttpConnector>(
                    &request,
                    Arc::new(crate::http_clients_cache::creators::SshConnectionCreator),
                    crate::consts::HTTP_DEFAULT_PORT.into(),
                    Some(Arc::new(ssh_credentials)),
                )
                .await;
        }

        let clients_cache = self.get_connections_cache();
        self.execute_with_retry::<my_ssh::SshAsyncChannel, SshHttpConnector>(
            &request,
            clients_cache,
            crate::consts::HTTP_DEFAULT_PORT.into(),
            Some(Arc::new(ssh_credentials)),
        )
        .await
    }
    pub(crate) fn get_connections_cache(&self) -> Arc<FlUrlHttpConnectionsCache> {
        match self.connections_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::CLIENTS_CACHED.clone(),
        }
    }

    fn compress_body(&mut self, body: Vec<u8>) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};

        if body.len() < 64 {
            return body;
        }

        if !self.headers.has_header("Content-Encoding") {
            self.headers.add("Content-Encoding", "gzip");
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_slice()).unwrap();
        let result = encoder.finish().unwrap();

        result
    }

    fn get_path_and_query_with_leading_slash(&self) -> String {
        let mut path_and_query = self.url_builder.get_path_and_query();
        // A URL with a query but no path yields "?a=b"; the request target must
        // start with "/", so we normalize it here.
        if path_and_query.starts_with('?') {
            path_and_query.insert(0, '/');
        }
        path_and_query
    }

    fn compile_request(
        &mut self,
        method: Method,
        body: HttpRequestBody,
        debug: Option<&mut String>,
    ) -> Result<CompiledHttpRequest, FlUrlError> {
        let result = match self.mode {
            FlUrlMode::H2 => CompiledHttpRequest::new_hyper(
                self.compile_hyper_request(method.clone(), body, debug)?,
                method,
            ),
            FlUrlMode::Http1NoHyper => CompiledHttpRequest::new_my_http_client(
                self.compile_non_hyper_request(method.clone(), body, debug)?,
                method,
            ),
            FlUrlMode::Http1Hyper => CompiledHttpRequest::new_hyper(
                self.compile_hyper_request(method.clone(), body, debug)?,
                method,
            ),
        };

        Ok(result)
    }

    fn compile_hyper_request(
        &mut self,
        method: Method,
        body: HttpRequestBody,
        debug: Option<&mut String>,
    ) -> Result<my_http_client::http::request::Request<Full<Bytes>>, FlUrlError> {
        if let Some(content_type) = body.get_content_type() {
            if !self.headers.has_header("Content-Type") {
                self.headers.add("Content-Type", content_type.as_str());
            }
        }

        let mut body = body.into_vec();

        if let Some(debug) = debug {
            self.compile_debug_info_with_body(debug, method.as_str(), &body);
        }

        if self.compress_body {
            body = self.compress_body(body);
        }

        let path_and_query = self.get_path_and_query_with_leading_slash();

        let mut result = match self.mode {
            FlUrlMode::H2 => {
                let scheme = if self.url_builder.get_scheme().is_https() {
                    "https"
                } else {
                    "http"
                };

                let uri = Uri::builder()
                    .authority(self.url_builder.get_host_port())
                    .path_and_query(path_and_query)
                    .scheme(scheme)
                    .build()?;
                my_http_client::http::request::Builder::new()
                    .version(Version::HTTP_2)
                    .method(method.clone())
                    .uri(uri)
            }
            _ => my_http_client::http::request::Builder::new()
                .method(method.clone())
                .uri(path_and_query),
        };

        for (key, value) in self.headers.iter() {
            result = result.header(key, value);
        }

        if !self.headers.has_host_header() {
            if !self.mode.is_h2() {
                result = result.header(
                    hyper::header::HOST.as_str(),
                    self.url_builder.get_host_port(),
                );
            }
        }

        if self.url_builder.is_unix_socket() {
            result = result.header(hyper::header::ACCEPT, "*/*");
        } else {
            if !self.headers.has_connection_header {
                if !self.do_not_reuse_connection {
                    result = result.header(hyper::header::CONNECTION.as_str(), "keep-alive");
                }
            }
        }

        let result = match result.body(Full::new(body.into())) {
            Ok(result) => result,
            Err(err) => {
                return Err(FlUrlError::ReadingHyperBodyError(format!(
                    "[{}]. '{}' '{}' Invalid getting fl_url body: {}",
                    method.as_str(),
                    self.url_builder.get_host_port(),
                    self.url_builder.get_path_and_query(),
                    err
                )));
            }
        };

        Ok(result)
    }

    fn compile_non_hyper_request(
        &mut self,
        method: Method,
        body: HttpRequestBody,
        debug: Option<&mut String>,
    ) -> Result<my_http_client::http1::MyHttpRequest, FlUrlError> {
        if let Some(content_type) = body.get_content_type() {
            if !self.headers.has_header("Content-Type") {
                self.headers.add("Content-Type", content_type.as_str());
            }
        }

        let mut body = body.into_vec();

        if let Some(debug) = debug {
            self.compile_debug_info_with_body(debug, method.as_str(), &body);
        }

        if self.compress_body {
            body = self.compress_body(body);
        }

        let path_and_query = self.get_path_and_query_with_leading_slash();

        let mut builder = MyHttpRequestBuilder::new(method, &path_and_query);

        if !self.headers.has_host_header() {
            builder.append_header("Host", self.url_builder.get_host_port());
        }

        if self.url_builder.is_unix_socket() {
            builder.append_header("Accept", "*/*");
        } else {
            if !self.headers.has_connection_header {
                if !self.do_not_reuse_connection {
                    builder.append_header("Connection", "keep-alive");
                }
            }
        }

        for header in self.headers.iter() {
            builder.append_header(header.0, header.1);
        }

        Ok(builder.build_with_body(body))
    }

    pub async fn get(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, HttpRequestBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn get_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::GET, HttpRequestBody::Empty, Some(request_debug_string))?;
        self.execute(request).await
    }

    pub async fn head(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, HttpRequestBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn post(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::POST, body.into(), None)?;
        self.execute(request).await
    }

    pub async fn post_with_debug(
        mut self,
        body: impl Into<HttpRequestBody>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = body.into();

        let request = self.compile_request(Method::POST, body, Some(request_debug_string))?;
        self.execute(request).await
    }

    #[deprecated(note = "Use `post` instead")]
    pub async fn post_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::POST, body, None)?;

        self.execute(request).await
    }

    pub async fn patch(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body.into(), None)?;
        self.execute(request).await
    }

    #[deprecated(note = "Use `patch` instead")]
    pub async fn patch_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::PATCH, body, None)?;

        self.execute(request).await
    }

    pub async fn put(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body.into(), None)?;
        self.execute(request).await
    }

    #[deprecated(note = "Use `put` instead")]
    pub async fn put_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::PUT, body, None)?;
        self.execute(request).await
    }

    pub async fn delete(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::DELETE, HttpRequestBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn delete_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::DELETE, HttpRequestBody::Empty, Some(request_debug_string))?;
        self.execute(request).await
    }
    fn compile_debug_info(&self, out: &mut String) {
        out.push_str("PathAndQuery: '");
        out.push_str(self.url_builder.get_path_and_query().as_str());
        out.push_str("'; Headers: '");
        out.push_str(self.headers.headers.as_str());
    }
    fn compile_debug_info_with_body(
        &self,
        request_debug_string: &mut String,
        method: &str,
        body: &[u8],
    ) {
        request_debug_string.push_str("[");
        request_debug_string.push_str(method);
        request_debug_string.push_str("] ");

        self.compile_debug_info(request_debug_string);

        if body.len() == 0 {
            return;
        }
        match std::str::from_utf8(body) {
            Ok(body_as_str) => {
                request_debug_string.push_str("Body: ");
                request_debug_string.push_str(body_as_str);
            }
            Err(_) => {
                request_debug_string.push_str("Body: ");
                request_debug_string.push_str(body.len().to_string().as_str());
                request_debug_string.push_str(" non string bytes");
            }
        }
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();
        self.compile_debug_info(&mut result);

        result
    }

    async fn get_connection_params<'s>(
        &'s self,
        default_port: Option<u16>,
        #[cfg(all(unix, feature = "with-ssh"))] ssh_credentials: Option<Arc<my_ssh::SshCredentials>>,
    ) -> ConnectionParams<'s> {
        let remote_endpoint = self.url_builder.get_remote_endpoint(default_port);

        #[cfg(all(unix, feature = "with-ssh"))]
        let ssh_session = match ssh_credentials.clone() {
            Some(ssh_credentials) => {
                let ssh_credentials = Arc::new(ssh_credentials);
                let ssh_session = my_ssh::SSH_SESSIONS_POOL
                    .get_or_create(&ssh_credentials)
                    .await;

                Some(ssh_session)
            }
            None => None,
        };

        ConnectionParams {
            mode: self.mode,
            remote_endpoint,
            host_header: self.headers.get_host_header_value(),
            client_certificate: self.client_cert.as_ref(),
            accept_invalid_certificate: self.accept_invalid_certificate,
            #[cfg(all(unix, feature = "with-ssh"))]
            ssh_session,
            reuse_connection_timeout_seconds: self.reuse_connection_timeout_sec,
        }
    }

    async fn execute_with_retry<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
    >(
        self,
        request: &CompiledHttpRequest,
        http_connection_resolver: Arc<dyn HttpConnectionResolver<TStream, TConnector>>,
        default_port: Option<u16>,
        #[cfg(all(unix, feature = "with-ssh"))] ssh_credentials: Option<Arc<my_ssh::SshCredentials>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if self.print_input_request {
            request.print_http_headers();
        }
        let mut attempt_no = 0;
        let max_retries = self.max_retries;
        let request_timeout = self.request_timeout;
        let params: ConnectionParams<'_> = self
            .get_connection_params(
                default_port,
                #[cfg(all(unix, feature = "with-ssh"))]
                ssh_credentials,
            )
            .await;

        loop {
            let connection = http_connection_resolver.get_http_connection(&params).await;

            let response = connection.do_request(request, request_timeout).await;

            match response {
                Ok(response) => {
                    let mut response =
                        FlUrlResponse::from_http1_response(self.url_builder, response);
                    response.set_body_read_timeout(self.response_body_timeout);
                    response.set_decompress_gzip(self.decompress_gzip_response);
                    // The connection stays checked out until the response body
                    // is fully consumed; the returner puts it back (or disposes
                    // it) at that point.
                    response.set_connection_returner(Box::new(
                        crate::http_clients_cache::PooledConnectionReturner {
                            resolver: http_connection_resolver.clone(),
                            connection,
                        },
                    ));
                    return Ok(response);
                }
                Err(err) => {
                    // A single timeout means a slow response, not a dead
                    // connection — the shared H2 client must survive it (its
                    // own consecutive-timeouts policy handles dead peers). Any
                    // other error evicts the connection from the pool; dropping
                    // the Arc disposes it.
                    if matches!(&err, my_http_client::MyHttpClientError::RequestTimeout(_)) {
                        drop(connection);
                    } else {
                        http_connection_resolver.drop_connection(connection).await;
                    }

                    if !error_is_safe_to_retry(&err, request) || attempt_no >= max_retries {
                        return Err(map_my_http_client_error(err));
                    }

                    attempt_no += 1;
                }
            }
        }
    }
}

/// Replay safety: fl-url's outer retry loop replays only idempotent requests.
/// Error kinds are NOT a reliable pre-wire signal across the three client
/// modes (e.g. in Http1NoHyper a `CanNotConnectToRemoteHost` can surface after
/// a POST already hit the wire, when the internal reconnect after a mid-flight
/// disconnect fails), so a non-idempotent request is never replayed here —
/// my-http-client's own retry loops already cover the genuinely-safe cases.
fn error_is_safe_to_retry(
    err: &my_http_client::MyHttpClientError,
    request: &CompiledHttpRequest,
) -> bool {
    match err {
        // The connection is consumed by the upgrade; a retry would just
        // re-trigger it.
        my_http_client::MyHttpClientError::UpgradedToWebSocket => false,
        _ => request.method_is_idempotent(),
    }
}

fn map_my_http_client_error(err: my_http_client::MyHttpClientError) -> FlUrlError {
    match err {
        my_http_client::MyHttpClientError::RequestTimeout(_) => FlUrlError::Timeout,
        other => FlUrlError::MyHttpClientError(other),
    }
}

#[cfg(test)]
mod test {

    use crate::FlUrl;

    #[tokio::test]
    async fn test_h1() {
        let mut fl_url_resp = FlUrl::new("https://jetdev.eu/img/logo.png")
            .do_not_reuse_connection()
            .get()
            .await
            .unwrap();

        println!("{}", fl_url_resp.get_status_code());

        let resp = fl_url_resp.get_body_as_slice().await.unwrap();
        println!("{}", resp.len());
    }

    #[tokio::test]
    async fn test_h2() {
        let mut fl_url_resp = FlUrl::new("https://jetdev.eu/img/logo.png")
            .update_mode(crate::fl_url::FlUrlMode::H2)
            .get()
            .await
            .unwrap();

        let resp = fl_url_resp.get_body_as_slice().await.unwrap();

        println!("{}", resp.len());
    }

    #[tokio::test]
    async fn test_head() {
        let mut fl_url_resp = FlUrl::new("https://jetdev.eu/img/logo.png")
            .head()
            .await
            .unwrap();

        let resp = fl_url_resp.get_body_as_slice().await.unwrap();

        println!("{}", resp.len());
    }

    #[test]
    fn execute_request_fills_url_headers_and_body_from_model() {
        use url_utils::macros::MyHttpInput;

        #[derive(MyHttpInput)]
        struct CreateUser {
            #[http_path(name = "orgId", description = "")]
            org_id: String,
            #[http_query(name = "notify", description = "")]
            notify: bool,
            #[http_header(name = "X-Api-Key", description = "")]
            api_key: String,
            #[http_body(name = "name", description = "")]
            name: String,
        }

        let model = CreateUser {
            org_id: "org-42".to_string(),
            notify: true,
            api_key: "secret".to_string(),
            name: "John".to_string(),
        };

        // Base host + static route prefix set by the caller, model fills the rest.
        let mut fl_url = FlUrl::new("https://api.example.com")
            .append_path_segment("api")
            .append_path_segment("users");

        let body = fl_url.fill_from_model(model).unwrap();

        // Static prefix + model path segment + model query param.
        assert_eq!(
            fl_url.url_builder.get_path_and_query(),
            "/api/users/org-42?notify=true"
        );

        // Model header field landed in FlUrlHeaders.
        assert!(fl_url
            .headers
            .iter()
            .any(|(name, value)| name == "X-Api-Key" && value == "secret"));

        // Body field serialized to JSON.
        match body {
            crate::body::HttpRequestBody::Json(bytes) => {
                let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(json["name"], "John");
            }
            _ => panic!("expected a JSON body"),
        }
    }

    #[test]
    fn execute_request_builds_multipart_body_with_random_boundary() {
        use url_utils::macros::MyHttpInput;

        // A form-data model is the only path where `FlUrlRnd` is actually used:
        // it supplies the random `multipart/form-data` boundary suffix.
        #[derive(MyHttpInput)]
        struct UploadForm {
            #[http_form_data(name = "title", description = "")]
            title: String,
            #[http_form_data(name = "count", description = "")]
            count: i32,
        }

        let model = UploadForm {
            title: "MyTitle".to_string(),
            count: 5,
        };

        let mut fl_url = FlUrl::new("https://api.example.com").append_path_segment("upload");

        let body = fl_url.fill_from_model(model).unwrap();

        // The model yields a FormData body, and its content type carries the
        // random boundary generated via `FlUrlRnd`.
        let content_type = body.get_content_type().unwrap().as_str().to_string();
        assert!(content_type.starts_with("multipart/form-data; boundary="));

        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .expect("content type must carry a boundary");
        // A non-empty random suffix was appended to the fixed boundary prefix.
        assert!(boundary.len() > "------DataFormBoundary".len());

        // The very boundary advertised in the content type delimits the body
        // bytes — i.e. the random string flowed all the way through.
        let text = String::from_utf8(body.into_vec()).unwrap();
        assert!(text.contains(boundary));
        assert!(text.contains("name=\"title\""));
        assert!(text.contains("MyTitle"));
        assert!(text.contains("name=\"count\""));
        assert!(text.contains('5'));
    }
}
