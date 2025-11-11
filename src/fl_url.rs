use bytes::Bytes;
use http_body_util::Full;
use hyper::Method;

use hyper::Uri;
use hyper::Version;
use my_http_client::MyHttpClientConnector;
use my_tls::tokio_rustls::client::TlsStream;

use rust_extensions::remote_endpoint::Scheme;
use rust_extensions::StrOrString;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

use super::FlUrlResponse;
use crate::body::FlUrlBody;
use crate::http_connectors::*;

use crate::http_clients_cache::*;

use crate::HttpClientResolver;

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

pub struct FlUrl {
    pub url_builder: UrlBuilder,
    pub headers: FlUrlHeaders,
    pub client_cert: Option<my_tls::ClientCertificate>,
    pub accept_invalid_certificate: bool,
    // If we are trying to reuse connection, but it was not used for this time, we will drop it
    pub not_used_connection_timeout: Duration,
    pub request_timeout: Duration,
    pub do_not_reuse_connection: bool,
    pub clients_cache: Option<Arc<FlUrlHttpClientsCache>>,
    pub compress_body: bool,
    pub print_input_request: bool,
    mode: FlUrlMode,
    #[cfg(feature = "with-ssh")]
    ssh_credentials: Option<my_ssh::SshCredentials>,
    #[cfg(feature = "with-ssh")]
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

        #[cfg(feature = "with-ssh")]
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

        #[cfg(not(feature = "with-ssh"))]
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
                } => panic!("To use ssh you need to enable with-ssh feature"),
            }
        };

        let result = Self {
            headers: FlUrlHeaders::new(),
            client_cert: None,
            url_builder: url,
            accept_invalid_certificate: false,
            do_not_reuse_connection: false,
            clients_cache: None,
            not_used_connection_timeout: Duration::from_secs(30),
            max_retries: 0,
            request_timeout: Duration::from_secs(10),
            print_input_request: false,
            compress_body: false,
            #[cfg(feature = "with-ssh")]
            ssh_credentials: credentials,
            #[cfg(feature = "with-ssh")]
            ssh_security_credentials_resolver: None,
            mode: Default::default(),
        };

        Ok(result)
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

    pub fn update_mode(mut self, mode: FlUrlMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_clients_cache(mut self, clients_cache: Arc<FlUrlHttpClientsCache>) -> Self {
        self.clients_cache = Some(clients_cache);
        self
    }

    pub fn with_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn print_input_request(mut self) -> Self {
        self.print_input_request = true;
        self
    }

    #[cfg(feature = "with-ssh")]
    pub fn set_ssh_security_credentials_resolver(
        mut self,
        resolver: Arc<dyn my_ssh::ssh_settings::SshSecurityCredentialsResolver + Send + Sync>,
    ) -> Self {
        self.ssh_security_credentials_resolver = Some(resolver);
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
    pub fn set_ssh_credentials(mut self, ssh_credentials: my_ssh::SshCredentials) -> Self {
        self.ssh_credentials = Some(ssh_credentials);
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
        self.request_timeout = timeout;
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

    async fn execute(
        self,
        request: my_http_client::http::request::Request<Full<Bytes>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        {
            if self.ssh_credentials.is_some() {
                return self.execute_ssh(request).await;
            }
        }

        let response = match self.url_builder.get_scheme() {
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
            #[cfg(not(unix))]
            Scheme::UnixSocket => {
                panic!("OS does not support unix sockets")
            }
            #[cfg(unix)]
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
            #[cfg(not(unix))]
            Scheme::UnixSocket => {
                panic!("OS does not support unix socket");
            }
        };

        Ok(response)
    }

    #[cfg(feature = "with-ssh")]
    async fn execute_ssh(
        mut self,
        request: my_http_client::http::request::Request<Full<Bytes>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let mut ssh_credentials = self.ssh_credentials.take().unwrap();

        if let Some(private_key_resolver) = self.ssh_security_credentials_resolver.take() {
            ssh_credentials = private_key_resolver
                .update_credentials(&ssh_credentials)
                .await;
        }

        let clients_cache = self.get_clients_cache();
        self.execute_with_retry::<my_ssh::SshAsyncChannel, SshHttpConnector, _>(
            &request,
            clients_cache.as_ref(),
            Some(Arc::new(ssh_credentials)),
        )
        .await
    }
    pub(crate) fn get_clients_cache(&self) -> Arc<FlUrlHttpClientsCache> {
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

    fn compile_request(
        &mut self,
        method: Method,
        body: FlUrlBody,
        debug: Option<&mut String>,
    ) -> Result<my_http_client::http::request::Request<Full<Bytes>>, FlUrlError> {
        if let Some(content_type) = body.get_content_type() {
            self.headers
                .add(hyper::header::CONTENT_TYPE.as_str(), content_type.as_str());
        }

        let mut body = body.into_vec();

        if let Some(debug) = debug {
            self.compile_debug_info_with_body(debug, method.as_str(), &body);
        }

        if self.compress_body {
            body = self.compress_body(body);
        }

        let path_and_query = self.url_builder.get_path_and_query();

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
                    .build()
                    .unwrap();
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
                result = result.header(hyper::header::HOST.as_str(), self.url_builder.get_host());
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

    pub async fn get(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::GET, FlUrlBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn get_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::GET, FlUrlBody::Empty, Some(request_debug_string))?;
        self.execute(request).await
    }

    pub async fn head(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::HEAD, FlUrlBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn post(mut self, body: impl Into<FlUrlBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::POST, body.into(), None)?;
        self.execute(request).await
    }

    pub async fn post_with_debug(
        mut self,
        body: impl Into<FlUrlBody>,
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
        let body = FlUrlBody::as_json(json);
        let request = self.compile_request(Method::POST, body, None)?;

        self.execute(request).await
    }

    pub async fn patch(mut self, body: impl Into<FlUrlBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PATCH, body.into(), None)?;
        self.execute(request).await
    }

    #[deprecated(note = "Use `patch` instead")]
    pub async fn patch_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = FlUrlBody::as_json(json);
        let request = self.compile_request(Method::PATCH, body, None)?;

        self.execute(request).await
    }

    pub async fn put(mut self, body: impl Into<FlUrlBody>) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::PUT, body.into(), None)?;
        self.execute(request).await
    }

    #[deprecated(note = "Use `put` instead")]
    pub async fn put_json(
        mut self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = FlUrlBody::as_json(json);
        let request = self.compile_request(Method::PUT, body, None)?;
        self.execute(request).await
    }

    pub async fn delete(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let request = self.compile_request(Method::DELETE, FlUrlBody::Empty, None)?;
        self.execute(request).await
    }

    pub async fn delete_with_debug(
        mut self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let request =
            self.compile_request(Method::DELETE, FlUrlBody::Empty, Some(request_debug_string))?;
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
                request_debug_string.push_str("non string bytes");
            }
        }
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();
        self.compile_debug_info(&mut result);

        result
    }

    async fn execute_with_retry<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
        TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
        THttpClientResolver: HttpClientResolver<TStream, TConnector>,
    >(
        mut self,
        request: &my_http_client::http::request::Request<Full<Bytes>>,
        http_client_resolver: &THttpClientResolver,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<Arc<my_ssh::SshCredentials>>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        if self.print_input_request {
            println!("{:?}", request.headers());
        }
        let mut attempt_no = 0;

        let client_cert = self.client_cert.take();

        loop {
            let http_client = http_client_resolver
                .get_http_client(
                    self.mode,
                    &self.url_builder,
                    self.headers.get_host_header_value(),
                    client_cert.as_ref(),
                    #[cfg(feature = "with-ssh")]
                    ssh_credentials.as_ref(),
                )
                .await;

            let response = http_client.do_request(request, self.request_timeout).await;

            match response {
                Ok(response) => {
                    let response = FlUrlResponse::from_http1_response(self.url_builder, response);

                    if response.drop_connection() {
                        http_client_resolver
                            .drop_http_client(
                                &response.url,
                                #[cfg(feature = "with-ssh")]
                                ssh_credentials.as_ref(),
                            )
                            .await;
                    }
                    return Ok(response);
                }
                Err(err) => {
                    http_client_resolver
                        .drop_http_client(
                            &self.url_builder,
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
}
