use bytes::Bytes;
use http_body_util::Full;
use hyper::Method;

use hyper::Uri;
use hyper::Version;
use my_http_client::http1::MyHttpRequestBuilder;
use my_http_client::MyHttpClientConnector;
#[cfg(feature = "_tls")]
use my_tls::tokio_rustls::client::TlsStream;

use rust_extensions::remote_endpoint::Scheme;
use rust_extensions::StrOrString;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

use super::FlUrlResponse;
use crate::body::HttpRequestBody;
use crate::non_wasm::compiled_http_request::{CompiledHttpRequest, RequestToExecute};
use crate::non_wasm::http_connectors::*;
use crate::non_wasm::model_body_stream::ModelBodyStream;

use crate::non_wasm::http_clients_cache::*;

use crate::HttpConnectionResolver;

use crate::FlUrlError;

use crate::FlUrlHeaders;

use my_http_utils::UrlBuilder;

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
    #[cfg(feature = "_tls")]
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
                    Some(crate::non_wasm::ssh::to_ssh_credentials(&ssh_remote_host)),
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
            #[cfg(feature = "_tls")]
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

    /// Only available with a TLS provider feature (`with-ring-tls` or
    /// `with-rust-tls`) — without one the crate does not link a TLS stack at all,
    /// so there is no certificate type to pass in.
    #[cfg(feature = "_tls")]
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

    /// Without a TLS provider feature this is inert: the request never reaches a
    /// TLS handshake because `https://` panics at execute time. It also needs
    /// `dangerous-tls` to have any effect at all — see that feature's docs.
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

    /// Pours a `my_http_utils` request model into this `FlUrl`: the model appends its
    /// path segments + query params to our `url_builder`, pushes its header fields
    /// into our `headers`, and hands over its body (which it consumes). The base
    /// host and any static route prefix must already be configured on `self`.
    fn fill_from_model(
        &mut self,
        model: impl my_http_utils::schema::client::THttpRequestBuilder,
    ) -> Result<HttpRequestBody, FlUrlError> {
        model.fill_url(&mut self.url_builder)?;
        model.fill_headers(&mut self.headers)?;
        // `get_body` consumes the model, so it must be the last thing we read.
        // The body is our own `HttpRequestBody` already — no conversion needed;
        // `compile_*_request` reads its (possibly dynamic, e.g. FormData boundary)
        // content type via `get_content_type()`. `FlUrlRnd` supplies the random
        // multipart boundary suffix (my-http-utils carries no RNG of its own).
        let body = model.get_body::<crate::body::FlUrlRnd>()?;
        Ok(body)
    }

    /// Executes an HTTP request described by a `my_http_utils` request model (any
    /// type deriving `my_http_utils::macros::MyHttpInput`). The model fills the URL
    /// path/query, headers, and body; `verb` selects the method. The base host and
    /// any static route prefix are configured on `self` beforehand via the usual
    /// builder methods (`append_path_segment`, `with_header`, …).
    ///
    /// For a parameter-less request, pass [`crate::EmptyRequestModel`] instead of
    /// deriving a dedicated model — the URL/headers already set on `self` are used
    /// as-is and body-carrying verbs send an empty body.
    ///
    /// `Get`/`Delete`/`Head` do not carry a body, so a body produced by the model
    /// is ignored for those verbs.
    ///
    /// A model with a `#[http_body_as_stream]` field goes down the streamed path
    /// instead — see [`Self::execute_model_stream`].
    pub async fn execute_request(
        mut self,
        verb: HttpVerb,
        model: impl my_http_utils::schema::client::THttpRequestBuilder,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = self.fill_from_model(model)?;

        if let HttpRequestBody::Stream(stream) = body {
            return self.execute_model_stream(verb, stream, None).await;
        }

        match verb {
            HttpVerb::Get => self.get().await,
            HttpVerb::Delete => self.delete().await,
            HttpVerb::Head => self.head().await,
            HttpVerb::Post => self.post(body).await,
            HttpVerb::Put => self.put(body).await,
            HttpVerb::Patch => self.patch(body).await,
        }
    }

    /// Same as [`Self::execute_request`], but dumps the compiled request — verb,
    /// path and query, headers and body — into `request_debug_string` before it goes
    /// on the wire. The dump is written for every verb, body-carrying or not.
    pub async fn execute_request_with_debug(
        mut self,
        verb: HttpVerb,
        model: impl my_http_utils::schema::client::THttpRequestBuilder,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = self.fill_from_model(model)?;

        if let HttpRequestBody::Stream(stream) = body {
            return self
                .execute_model_stream(verb, stream, Some(request_debug_string))
                .await;
        }

        match verb {
            HttpVerb::Get => self.get_with_debug(request_debug_string).await,
            HttpVerb::Delete => self.delete_with_debug(request_debug_string).await,
            HttpVerb::Head => self.head_with_debug(request_debug_string).await,
            HttpVerb::Post => self.post_with_debug(body, request_debug_string).await,
            HttpVerb::Put => self.put_with_debug(body, request_debug_string).await,
            HttpVerb::Patch => self.patch_with_debug(body, request_debug_string).await,
        }
    }

    /// Sends a model whose body is a `#[http_body_as_stream]` field: the chunks the
    /// application writes into the stream are pulled out of it and written to the
    /// socket as they arrive, so the payload is never materialized.
    ///
    /// The framing comes from the stream itself — the `content_length` given to
    /// `HttpBodyAsStream::create` becomes `Content-Length`, and `None` goes out
    /// chunked. Everything else that applies to a streamed body applies here too
    /// (see [`Self::execute_streamed`]): no `compress()`, no retries, the timeout
    /// covers the whole upload.
    async fn execute_model_stream(
        self,
        verb: HttpVerb,
        stream: my_http_utils::http_input::HttpBodyAsStream,
        debug: Option<&mut String>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        // A streamed payload on a body-less verb is a mistake in the call, not
        // something to drop quietly the way a materialized body is: the application
        // is already writing into the stream, and nothing would ever read it.
        let method = match verb {
            HttpVerb::Post => Method::POST,
            HttpVerb::Put => Method::PUT,
            HttpVerb::Patch => Method::PATCH,
            HttpVerb::Get | HttpVerb::Delete | HttpVerb::Head => {
                return Err(FlUrlError::RequestBuild(format!(
                    "{:?} carries no body, but the model streams one (#[http_body_as_stream])",
                    verb
                )))
            }
        };

        // `empty()` (a model built to be parsed by a server, never sent) and a stream
        // whose reader was already taken both land here, and the message says which.
        let reader = stream.get_body_reader().map_err(|err| {
            FlUrlError::RequestBuild(format!(
                "#[http_body_as_stream] model has no body to send: {}",
                err
            ))
        })?;

        let content_length = reader.get_content_length().map(|len| len as usize);

        self.execute_streamed_impl(
            method,
            ModelBodyStream::new(reader),
            content_length,
            debug,
        )
        .await
    }

    async fn execute(self, request: RequestToExecute) -> Result<FlUrlResponse, FlUrlError> {
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
                        request,
                        Arc::new(crate::non_wasm::http_clients_cache::creators::HttpConnectionCreator),
                        crate::consts::HTTP_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();
                    self.execute_with_retry::<TcpStream, HttpConnector>(
                        request,
                        clients_cache,
                        crate::consts::HTTP_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                }
            }
            #[cfg(not(feature = "_tls"))]
            Scheme::Https => {
                panic!(
                    "FlUrl does not support https: it is compiled without a TLS provider feature. Enable 'with-ring-tls' (ring) or 'with-rust-tls' (pure Rust). Url: {}",
                    self.url_builder
                )
            }
            #[cfg(feature = "_tls")]
            Scheme::Https => {
                if self.do_not_reuse_connection {
                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector>(
                        request,
                        Arc::new(crate::non_wasm::http_clients_cache::creators::HttpsConnectionCreator),
                        crate::consts::HTTPS_DEFAULT_PORT.into(),
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();

                    self.execute_with_retry::<TlsStream<TcpStream>, HttpsConnector>(
                        request,
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
                        request,
                        Arc::new(crate::non_wasm::http_clients_cache::creators::UnixSocketHttpClientCreator),
                        None,
                        #[cfg(all(unix, feature = "with-ssh"))]
                        None,
                    )
                    .await?
                } else {
                    let clients_cache = self.get_connections_cache();

                    self.execute_with_retry::<UnixSocketStream, UnixSocketConnector>(
                        request,
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
        request: RequestToExecute,
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
                    request,
                    Arc::new(crate::non_wasm::http_clients_cache::creators::SshConnectionCreator),
                    crate::consts::HTTP_DEFAULT_PORT.into(),
                    Some(Arc::new(ssh_credentials)),
                )
                .await;
        }

        let clients_cache = self.get_connections_cache();
        self.execute_with_retry::<my_ssh::SshAsyncChannel, SshHttpConnector>(
            request,
            clients_cache,
            crate::consts::HTTP_DEFAULT_PORT.into(),
            Some(Arc::new(ssh_credentials)),
        )
        .await
    }
    pub(crate) fn get_connections_cache(&self) -> Arc<FlUrlHttpConnectionsCache> {
        match self.connections_cache.as_ref() {
            Some(cache) => cache.clone(),
            None => crate::non_wasm::CLIENTS_CACHED.clone(),
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

                // H1 puts the socket path into the Host header and gets away with it;
                // h2 can not, because ':authority' is parsed as a real authority and a
                // path ends one at its first '/'. The caller's own Host header wins,
                // otherwise the placeholder — the socket to open is already known to
                // the connector, so the authority is only what the server sees.
                #[cfg(unix)]
                let authority = if self.url_builder.is_unix_socket() {
                    self.headers
                        .get_host_header_value()
                        .unwrap_or(crate::consts::UNIX_SOCKET_AUTHORITY)
                } else {
                    self.url_builder.get_host_port()
                };

                #[cfg(not(unix))]
                let authority = self.url_builder.get_host_port();

                let uri = Uri::builder()
                    .authority(authority)
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
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn get_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::GET, HttpRequestBody::Empty, Some(request_debug_string))?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn head(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, HttpRequestBody::Empty, None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn head_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(
            Method::HEAD,
            HttpRequestBody::Empty,
            Some(request_debug_string),
        )?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn post(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::POST, body.into(), None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn post_with_debug(
        mut self,
        body: impl Into<HttpRequestBody>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = body.into();

        let request = self.compile_request(Method::POST, body, Some(request_debug_string))?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    /// POSTs a body that is produced as a stream instead of living in memory as a
    /// whole. See [`Self::execute_streamed`] for what applies to every streamed
    /// request — framing, retries, timeouts.
    ///
    /// ```no_run
    /// # async fn doc() -> Result<(), flurl::FlUrlError> {
    /// use my_http_client::RequestBodyStream;
    ///
    /// let (publisher, body) = RequestBodyStream::new(4);
    ///
    /// tokio::spawn(async move {
    ///     for chunk in 0..10u8 {
    ///         // Err means the request is over — nothing else can be published
    ///         if publisher.publish(vec![chunk; 64 * 1024]).await.is_err() {
    ///             break;
    ///         }
    ///     }
    ///     // dropping the publisher is what ends the body
    /// });
    ///
    /// let response = flurl::FlUrl::new("https://api.example.com")
    ///     .append_path_segment("upload")
    ///     .set_timeout(std::time::Duration::from_secs(600))
    ///     // None: the size is unknown, so the body goes out chunked
    ///     .post_request_streamed(body, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn post_request_streamed<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed(Method::POST, body, content_length)
            .await
    }

    /// Same as [`Self::post_request_streamed`], with the request head dumped into
    /// `request_debug_string` — the streamed payload itself is not printed. See
    /// [`Self::execute_streamed_with_debug`].
    pub async fn post_request_streamed_with_debug<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed_with_debug(Method::POST, body, content_length, request_debug_string)
            .await
    }

    /// PUTs a body that is produced as a stream. See [`Self::execute_streamed`].
    ///
    /// An upload whose size is known — a file being sent somewhere — is the case for
    /// passing a length rather than letting it go out chunked:
    ///
    /// ```no_run
    /// # async fn doc(len: usize, body: my_http_client::RequestBodyStream<Vec<u8>>)
    /// # -> Result<(), flurl::FlUrlError> {
    /// let response = flurl::FlUrl::new("https://api.example.com")
    ///     .append_path_segment("files")
    ///     .append_path_segment("archive.tar")
    ///     .set_timeout(std::time::Duration::from_secs(600))
    ///     // `len` and the producer of `body` must come from one source
    ///     .put_request_streamed(body, Some(len))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put_request_streamed<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed(Method::PUT, body, content_length)
            .await
    }

    /// Same as [`Self::put_request_streamed`], with the request head dumped into
    /// `request_debug_string`. See [`Self::execute_streamed_with_debug`].
    pub async fn put_request_streamed_with_debug<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed_with_debug(Method::PUT, body, content_length, request_debug_string)
            .await
    }

    /// PATCHes a body that is produced as a stream. See [`Self::execute_streamed`].
    pub async fn patch_request_streamed<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed(Method::PATCH, body, content_length)
            .await
    }

    /// Same as [`Self::patch_request_streamed`], with the request head dumped into
    /// `request_debug_string`. See [`Self::execute_streamed_with_debug`].
    pub async fn patch_request_streamed_with_debug<TBody>(
        self,
        body: TBody,
        content_length: Option<usize>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed_with_debug(Method::PATCH, body, content_length, request_debug_string)
            .await
    }

    /// Sends `body` as a stream under `method` — a [`my_http_client::RequestBodyStream`]
    /// fed by a publisher, a proxied `hyper::body::Incoming`, a `StreamBody` over a
    /// file reader, anything implementing `hyper::body::Body<Data = Bytes>`. Peak
    /// memory is one chunk plus whatever the producer buffers, not the size of the
    /// payload.
    ///
    /// This path is hyper HTTP/1.1 only: the mode is pinned to
    /// [`FlUrlMode::Http1Hyper`] regardless of [`Self::update_mode`], because the own
    /// HTTP/1.1 implementation serializes a request into one buffer and the h2 client
    /// has no streaming entry point.
    ///
    /// **Framing** is what `content_length` picks. HTTP/1.1 delimits a request body
    /// exactly two ways (RFC 9112 §6), and these are they:
    ///
    /// * `None` → `Transfer-Encoding: chunked`. The length never has to be known, and
    ///   every HTTP/1.1 recipient is required to understand chunked, so this is the
    ///   default a streamed body wants.
    /// * `Some(n)` → `Content-Length: n`, no chunked. For endpoints that refuse a
    ///   chunked request body, and for anything that has to know the size before it
    ///   starts reading.
    ///
    /// With `Some(n)` the body must then deliver **exactly** `n` bytes. That is the
    /// protocol's rule, not fl-url's: a short body makes the message incomplete, and
    /// extra bytes would be read as the start of the next request on the same
    /// connection. A stream that ends early therefore fails the request
    /// (`"user body write aborted"`) instead of putting a truncated payload on the
    /// wire — so `n` and the producer must come from one source (a file's metadata and
    /// that same file), never be computed twice.
    ///
    /// `content_length` is the single source of the framing, so it overrides a
    /// `Content-Length` added with [`Self::with_header`] in both directions: `Some(n)`
    /// replaces such a header (never emits a second one, which would be a protocol
    /// violation), and `None` removes it, because a body of unknown size must not
    /// claim a length it may not deliver.
    ///
    /// Three builder knobs do not apply, and none of them fails quietly:
    ///
    /// * [`Self::compress`] gzips the body as one buffer, which is exactly what
    ///   streaming avoids → [`FlUrlError::StreamedBodyCanNotBeCompressed`].
    /// * [`Self::with_retries`] is ignored: the payload is consumed as it is sent, so
    ///   the request is attempted exactly once. Rebuilding the stream and calling
    ///   again is the caller's decision — it owns the source data.
    /// * [`Self::set_timeout`] covers the **whole** call, upload included, not just
    ///   the wait for the response head. The 10s default is far too short for a real
    ///   upload; set it to the size of the transfer you expect.
    pub async fn execute_streamed<TBody>(
        self,
        method: Method,
        body: TBody,
        content_length: Option<usize>,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed_impl(method, body, content_length, None)
            .await
    }

    /// Same as [`Self::execute_streamed`], with the request head — verb, path and
    /// query, headers — dumped into `request_debug_string` before it goes on the wire.
    ///
    /// The payload is **not** in the dump: a streamed body exists only as it is
    /// written to the socket, so printing it would mean buffering the very thing
    /// streaming avoids.
    pub async fn execute_streamed_with_debug<TBody>(
        self,
        method: Method,
        body: TBody,
        content_length: Option<usize>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        self.execute_streamed_impl(method, body, content_length, Some(request_debug_string))
            .await
    }

    async fn execute_streamed_impl<TBody>(
        mut self,
        method: Method,
        body: TBody,
        content_length: Option<usize>,
        debug: Option<&mut String>,
    ) -> Result<FlUrlResponse, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        if self.compress_body {
            return Err(FlUrlError::StreamedBodyCanNotBeCompressed);
        }

        self.mode = FlUrlMode::Http1Hyper;

        if let Some(debug) = debug {
            self.compile_debug_info_streamed(debug, method.as_str());
        }

        let request = self.compile_streamed_request(method, body)?;

        self.execute(RequestToExecute::streamed(request, content_length))
            .await
    }

    /// Builds the request head for [`Self::execute_streamed`] and erases the body to
    /// the trait object the connection carries. Mirrors the header work of
    /// `compile_hyper_request`, minus everything that needs the body in hand:
    /// no `Content-Type` derived from the payload (a stream carries none — set it with
    /// [`Self::with_header`]), no debug dump of the body, no compression.
    fn compile_streamed_request<TBody>(
        &mut self,
        method: Method,
        body: TBody,
    ) -> Result<my_http_client::HyperRequest, FlUrlError>
    where
        TBody: hyper::body::Body<Data = Bytes> + Send + Sync + 'static,
        TBody::Error: std::fmt::Display,
    {
        let path_and_query = self.get_path_and_query_with_leading_slash();

        // Http1Hyper only, so the origin-form target is the right one — no absolute
        // URI + :authority the way the h2 branch builds it.
        let mut result = my_http_client::http::request::Builder::new()
            .method(method.clone())
            .uri(path_and_query);

        for (key, value) in self.headers.iter() {
            result = result.header(key, value);
        }

        if !self.headers.has_host_header() {
            result = result.header(
                hyper::header::HOST.as_str(),
                self.url_builder.get_host_port(),
            );
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

        let body = http_body_util::BodyExt::boxed(http_body_util::BodyExt::map_err(
            body,
            |err| err.to_string(),
        ));

        match result.body(body) {
            Ok(result) => Ok(result),
            Err(err) => Err(FlUrlError::ReadingHyperBodyError(format!(
                "[{}]. '{}' '{}' Invalid getting fl_url streamed body: {}",
                method.as_str(),
                self.url_builder.get_host_port(),
                self.url_builder.get_path_and_query(),
                err
            ))),
        }
    }

    #[deprecated(note = "Use `post` instead")]
    pub async fn post_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::POST, body, None)?;

        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn patch(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body.into(), None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn patch_with_debug(
        mut self,
        body: impl Into<HttpRequestBody>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body.into(), Some(request_debug_string))?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    #[deprecated(note = "Use `patch` instead")]
    pub async fn patch_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::PATCH, body, None)?;

        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn put(mut self, body: impl Into<HttpRequestBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body.into(), None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn put_with_debug(
        mut self,
        body: impl Into<HttpRequestBody>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body.into(), Some(request_debug_string))?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    #[deprecated(note = "Use `put` instead")]
    pub async fn put_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        let request = self.compile_request(Method::PUT, body, None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn delete(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::DELETE, HttpRequestBody::Empty, None)?;
        self.execute(RequestToExecute::Compiled(request)).await
    }

    pub async fn delete_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::DELETE, HttpRequestBody::Empty, Some(request_debug_string))?;
        self.execute(RequestToExecute::Compiled(request)).await
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

    /// Debug dump for a streamed request: the head only. There is no body line — a
    /// streamed payload exists only as it is written to the socket, so printing it
    /// would mean buffering the very thing streaming avoids.
    fn compile_debug_info_streamed(&self, request_debug_string: &mut String, method: &str) {
        request_debug_string.push_str("[");
        request_debug_string.push_str(method);
        request_debug_string.push_str("] ");

        self.compile_debug_info(request_debug_string);
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
            #[cfg(feature = "_tls")]
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
        mut request: RequestToExecute,
        http_connection_resolver: Arc<dyn HttpConnectionResolver<TStream, TConnector>>,
        default_port: Option<u16>,
        #[cfg(all(unix, feature = "with-ssh"))] ssh_credentials: Option<Arc<my_ssh::SshCredentials>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if self.print_input_request {
            request.print_http_headers();
        }
        let mut attempt_no = 0;
        // A streamed body is consumed as it is sent, so there is nothing left to
        // replay — `with_retries` does not apply to it, whatever it was set to.
        let max_retries = if request.is_streamed() {
            0
        } else {
            self.max_retries
        };
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

            let response = match &mut request {
                RequestToExecute::Compiled(request) => {
                    connection.do_request(request, request_timeout).await
                }
                RequestToExecute::Streamed {
                    request,
                    content_size,
                    ..
                } => match request.take() {
                    Some(request) => {
                        connection
                            .do_streamed_request(request, *content_size, request_timeout)
                            .await
                    }
                    // Unreachable while max_retries is pinned to 0 above; kept as an
                    // error rather than an unwrap so a future change to the retry
                    // policy can not silently resend a half-consumed body.
                    None => Err(my_http_client::MyHttpClientError::CanNotExecuteRequest(
                        "A streamed request body has already been consumed and can not be replayed"
                            .to_string(),
                    )),
                },
            };

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
                        crate::non_wasm::http_clients_cache::PooledConnectionReturner {
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

                    if !error_is_safe_to_retry(&err, &request) || attempt_no >= max_retries {
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
    request: &RequestToExecute,
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

    #[cfg(feature = "_tls")]
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

    #[cfg(feature = "_tls")]
    #[tokio::test]
    async fn test_h2() {
        let mut fl_url_resp = FlUrl::new("https://jetdev.eu/img/logo.png")
            .update_mode(crate::non_wasm::fl_url::FlUrlMode::H2)
            .get()
            .await
            .unwrap();

        let resp = fl_url_resp.get_body_as_slice().await.unwrap();

        println!("{}", resp.len());
    }

    #[cfg(feature = "_tls")]
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
        use my_http_utils::macros::MyHttpInput;

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
        use my_http_utils::macros::MyHttpInput;

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

    // Compile-only: the "no model" example must type-check. `EmptyRequestModel`
    // is the parameter-less stand-in — no model type has to be derived or named.
    #[allow(dead_code)]
    fn readme_no_model_example_compiles() {
        use crate::{EmptyRequestModel, FlUrlError, FlUrlResponse, HttpVerb};

        async fn _call() -> Result<FlUrlResponse, FlUrlError> {
            FlUrl::new("https://api.example.com")
                .append_path_segment("health")
                .execute_request(HttpVerb::Get, EmptyRequestModel)
                .await
        }
    }

    #[test]
    fn execute_request_keeps_headers_added_before_it() {
        use my_http_utils::macros::MyHttpInput;

        #[derive(MyHttpInput)]
        struct Model {
            #[http_header(name = "X-Api-Key", description = "")]
            api_key: String,
            #[http_body(name = "name", description = "")]
            name: String,
        }

        let model = Model {
            api_key: "secret".to_string(),
            name: "John".to_string(),
        };

        // Two headers set on the builder BEFORE the model is poured in.
        let mut fl_url = FlUrl::new("https://api.example.com")
            .with_header("Authorization", "Bearer token")
            .with_header("X-Trace", "abc");

        fl_url.fill_from_model(model).unwrap();

        let has = |n: &str, v: &str| {
            fl_url
                .headers
                .iter()
                .any(|(name, value)| name == n && value == v)
        };

        // The headers added before execute_request/fill_from_model survive...
        assert!(has("Authorization", "Bearer token"));
        assert!(has("X-Trace", "abc"));
        // ...right alongside the header field the model pushes in.
        assert!(has("X-Api-Key", "secret"));
    }

    /// Every verb goes through `compile_request`, which is what the `*_with_debug`
    /// methods hand the debug string to — so the dump carries the verb, the target
    /// and, for a body-carrying verb, the payload.
    #[test]
    fn every_verb_dumps_the_request_into_the_debug_string() {
        use crate::body::HttpRequestBody;
        use hyper::Method;

        let cases = [
            (Method::GET, None),
            (Method::HEAD, None),
            (Method::DELETE, None),
            (Method::POST, Some("{\"a\":1}")),
            (Method::PUT, Some("{\"b\":2}")),
            (Method::PATCH, Some("{\"c\":3}")),
        ];

        for (method, body) in cases {
            let mut fl_url = FlUrl::new("https://api.example.com")
                .append_path_segment("users")
                .append_query_param("notify", Some("true"))
                .with_header("X-Api-Key", "secret");

            let request_body = match body {
                Some(body) => HttpRequestBody::from_raw_data(
                    body.as_bytes().to_vec(),
                    Some("application/json".into()),
                ),
                None => HttpRequestBody::Empty,
            };

            let mut debug = String::new();
            fl_url
                .compile_request(method.clone(), request_body, Some(&mut debug))
                .unwrap();

            assert!(
                debug.starts_with(&format!("[{}] ", method.as_str())),
                "[{}] dump must open with the verb: {}",
                method.as_str(),
                debug
            );
            assert!(
                debug.contains("/users?notify=true"),
                "[{}] dump must carry the target: {}",
                method.as_str(),
                debug
            );
            assert!(
                debug.contains("X-Api-Key"),
                "[{}] dump must carry the headers: {}",
                method.as_str(),
                debug
            );

            match body {
                Some(body) => assert!(
                    debug.contains(&format!("Body: {}", body)),
                    "[{}] dump must carry the body: {}",
                    method.as_str(),
                    debug
                ),
                // A bodyless verb has nothing to print — and prints nothing.
                None => assert!(
                    !debug.contains("Body: "),
                    "[{}] must not invent a body: {}",
                    method.as_str(),
                    debug
                ),
            }
        }
    }

    /// A streamed body cannot be dumped — it exists only as it is written to the
    /// socket — so the dump is the head and nothing else.
    #[test]
    fn a_streamed_request_dumps_the_head_without_a_body() {
        let fl_url = FlUrl::new("https://api.example.com")
            .append_path_segment("upload")
            .with_header("X-Api-Key", "secret");

        let mut debug = String::new();
        fl_url.compile_debug_info_streamed(&mut debug, "POST");

        assert!(debug.starts_with("[POST] "), "{}", debug);
        assert!(debug.contains("/upload"), "{}", debug);
        assert!(debug.contains("X-Api-Key"), "{}", debug);
        assert!(!debug.contains("Body"), "{}", debug);
    }
}
