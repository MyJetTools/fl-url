use std::sync::Arc;
use std::time::Duration;

use rust_extensions::remote_endpoint::Scheme;
use rust_extensions::StrOrString;
use my_http_utils::UrlBuilder;

use crate::body::HttpRequestBody;
use crate::wasm::fetch::{execute_fetch, get_origin};
use crate::wasm::{FlUrlHttpConnectionsCache, FlUrlResponse};
use crate::{FlUrlError, FlUrlHeaders};

/// Kept for API parity with the native backend. Under wasm the browser negotiates
/// the HTTP version, so the mode is stored but otherwise ignored.
#[derive(Debug, Clone, Copy)]
pub enum FlUrlMode {
    H2,
    Http1NoHyper,
    Http1Hyper,
}

impl FlUrlMode {
    pub fn is_h2(&self) -> bool {
        matches!(self, Self::H2)
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

/// The wasm `FlUrl`. Same fluent, builder-style API as the native backend, but
/// backed by the browser `fetch`. Knobs that only make sense for the native
/// transport (connection pool, TLS validation, HTTP mode, connection reuse) are
/// stored for signature parity and otherwise ignored — the browser handles them.
pub struct FlUrl {
    pub url_builder: UrlBuilder,
    pub headers: FlUrlHeaders,
    pub accept_invalid_certificate: bool,
    pub not_used_connection_timeout: Duration,
    pub request_timeout: Duration,
    pub response_body_timeout: Option<Duration>,
    pub do_not_reuse_connection: bool,
    pub connections_cache: Option<Arc<FlUrlHttpConnectionsCache>>,
    pub compress_body: bool,
    pub decompress_gzip_response: bool,
    pub print_input_request: bool,
    pub reuse_connection_timeout_sec: i64,
    mode: FlUrlMode,
    max_retries: usize,
}

impl FlUrl {
    pub fn new<'s>(url: impl Into<StrOrString<'s>>) -> Self {
        Self::try_new(url).unwrap()
    }

    pub fn try_new<'s>(url: impl Into<StrOrString<'s>>) -> Result<Self, FlUrlError> {
        let url: StrOrString<'s> = url.into();

        // A URL that is not an absolute `http(s)` URL (e.g. `/api/xxx`) carries no
        // scheme/host of its own. Under wasm we resolve it against the current page
        // (or worker) origin, the same way the browser resolves a relative `fetch`:
        // `/api/xxx` -> `https://my-host/api/xxx`.
        let resolved_url = if needs_origin_prefix(url.as_str()) {
            Some(format!("{}{}", get_origin()?, url.as_str()))
        } else {
            None
        };
        let url_str = resolved_url.as_deref().unwrap_or_else(|| url.as_str());

        let endpoint =
            rust_extensions::remote_endpoint::RemoteEndpointHostString::try_parse(url_str)
                .map_err(FlUrlError::InvalidUrl)?;

        let url_builder = match endpoint {
            rust_extensions::remote_endpoint::RemoteEndpointHostString::Direct(_) => {
                UrlBuilder::new(url_str)
            }
            rust_extensions::remote_endpoint::RemoteEndpointHostString::ViaSsh { .. } => {
                return Err(FlUrlError::UnsupportedScheme(
                    "SSH tunneling is not supported under wasm".to_string(),
                ));
            }
        };

        Ok(Self {
            url_builder,
            headers: FlUrlHeaders::new(),
            accept_invalid_certificate: false,
            not_used_connection_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            response_body_timeout: None,
            do_not_reuse_connection: false,
            connections_cache: None,
            compress_body: false,
            decompress_gzip_response: false,
            print_input_request: false,
            reuse_connection_timeout_sec: 120,
            mode: Default::default(),
            max_retries: 0,
        })
    }

    pub fn compress(mut self) -> Self {
        self.compress_body = true;
        self
    }

    /// No-op under wasm: the browser negotiates `Accept-Encoding` and
    /// transparently decompresses the response, so there is nothing to do here.
    /// Kept for API parity.
    pub fn accept_gzip(self) -> Self {
        self
    }

    pub fn set_not_used_connection_timeout(mut self, timeout: Duration) -> Self {
        self.not_used_connection_timeout = timeout;
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
    /// idempotent methods are replayed (a POST/PATCH that may have reached the
    /// server is never re-sent).
    pub fn with_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn print_input_request(mut self) -> Self {
        self.print_input_request = true;
        self
    }

    pub fn set_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Stored for API parity. Under wasm the body is buffered by `fetch` in one
    /// `arrayBuffer()` call, so there is no separate body-read timeout to enforce.
    pub fn set_response_body_timeout(mut self, timeout: Duration) -> Self {
        self.response_body_timeout = Some(timeout);
        self
    }

    /// No-op under wasm (the browser owns connection reuse). Kept for API parity.
    pub fn do_not_reuse_connection(mut self) -> Self {
        self.do_not_reuse_connection = true;
        self
    }

    /// No-op under wasm: server-certificate validation is controlled by the
    /// browser and can not be relaxed from JS. Kept for API parity.
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
        }

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
    /// into our `headers`, and hands over its body (which it consumes).
    fn fill_from_model(
        &mut self,
        model: impl my_http_utils::schema::client::THttpRequestBuilder,
    ) -> Result<HttpRequestBody, FlUrlError> {
        model.fill_url(&mut self.url_builder)?;
        model.fill_headers(&mut self.headers)?;
        let body = model.get_body::<crate::body::FlUrlRnd>()?;
        Ok(body)
    }

    /// Executes an HTTP request described by a `my_http_utils` request model (any type
    /// deriving `my_http_utils::macros::MyHttpInput`). Mirrors the native backend.
    ///
    /// `Get`/`Delete`/`Head` do not carry a body, so a body produced by the model
    /// is ignored for those verbs.
    pub async fn execute_request(
        mut self,
        verb: HttpVerb,
        model: impl my_http_utils::schema::client::THttpRequestBuilder,
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

    pub async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        self.run("GET", true, HttpRequestBody::Empty, None).await
    }

    pub async fn get_with_debug(
        self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run("GET", true, HttpRequestBody::Empty, Some(request_debug_string))
            .await
    }

    pub async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        self.run("HEAD", true, HttpRequestBody::Empty, None).await
    }

    pub async fn post(
        self,
        body: impl Into<HttpRequestBody>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run("POST", false, body.into(), None).await
    }

    pub async fn post_with_debug(
        self,
        body: impl Into<HttpRequestBody>,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run("POST", false, body.into(), Some(request_debug_string))
            .await
    }

    #[deprecated(note = "Use `post` instead")]
    pub async fn post_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        self.run("POST", false, body, None).await
    }

    pub async fn patch(
        self,
        body: impl Into<HttpRequestBody>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run("PATCH", false, body.into(), None).await
    }

    #[deprecated(note = "Use `patch` instead")]
    pub async fn patch_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        self.run("PATCH", false, body, None).await
    }

    pub async fn put(
        self,
        body: impl Into<HttpRequestBody>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run("PUT", true, body.into(), None).await
    }

    #[deprecated(note = "Use `put` instead")]
    pub async fn put_json(
        self,
        json: &impl serde::Serialize,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = HttpRequestBody::try_as_json(json)?;
        self.run("PUT", true, body, None).await
    }

    pub async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        self.run("DELETE", true, HttpRequestBody::Empty, None).await
    }

    pub async fn delete_with_debug(
        self,
        request_debug_string: &mut String,
    ) -> Result<FlUrlResponse, FlUrlError> {
        self.run(
            "DELETE",
            true,
            HttpRequestBody::Empty,
            Some(request_debug_string),
        )
        .await
    }

    async fn run(
        mut self,
        method: &str,
        idempotent: bool,
        body: HttpRequestBody,
        debug: Option<&mut String>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        match self.url_builder.get_scheme() {
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
            Scheme::UnixSocket => {
                return Err(FlUrlError::UnsupportedScheme(
                    "Unix sockets are not supported under wasm".to_string(),
                ))
            }
            _ => {}
        }

        let body_bytes = self.compile(method, body, debug);
        let url = self.url_builder.to_string();
        let header_list: Vec<(String, String)> = self
            .headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        // `request_timeout` bounds the request→headers round-trip; the (optional)
        // `response_body_timeout` bounds the body read separately, mirroring the
        // native backend.
        let request_timeout_millis = duration_to_millis(self.request_timeout);
        let body_timeout_millis = self.response_body_timeout.and_then(duration_to_millis);

        let mut attempt_no = 0;
        let (response, controller) = loop {
            let attempt_body = if body_bytes.is_empty() {
                None
            } else {
                Some(body_bytes.as_slice())
            };

            match execute_fetch(
                &url,
                method,
                &header_list,
                attempt_body,
                request_timeout_millis,
                self.print_input_request,
            )
            .await
            {
                Ok(pair) => break pair,
                Err(err) => {
                    if !error_is_safe_to_retry(&err, idempotent) || attempt_no >= self.max_retries {
                        return Err(err);
                    }
                    attempt_no += 1;
                }
            }
        };

        Ok(FlUrlResponse::new(
            self.url_builder,
            response,
            controller,
            body_timeout_millis,
        ))
    }

    /// Fills in `Content-Type` (from the body) if absent, materializes the body,
    /// optionally writes the debug string and gzip-compresses the body — mirroring
    /// the native `compile_request`.
    fn compile(
        &mut self,
        method: &str,
        body: HttpRequestBody,
        debug: Option<&mut String>,
    ) -> Vec<u8> {
        if let Some(content_type) = body.get_content_type() {
            if !self.headers.has_header("Content-Type") {
                self.headers.add("Content-Type", content_type.as_str());
            }
        }

        let mut bytes = body.into_vec();

        if let Some(debug) = debug {
            self.fill_debug(debug, method, &bytes);
        }

        if self.compress_body {
            bytes = self.compress_body(bytes);
        }

        bytes
    }

    fn compress_body(&mut self, body: Vec<u8>) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;

        if body.len() < 64 {
            return body;
        }

        if !self.headers.has_header("Content-Encoding") {
            self.headers.add("Content-Encoding", "gzip");
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_slice()).unwrap();
        encoder.finish().unwrap()
    }

    fn fill_debug(&self, out: &mut String, method: &str, body: &[u8]) {
        out.push_str("[");
        out.push_str(method);
        out.push_str("] PathAndQuery: '");
        out.push_str(self.url_builder.get_path_and_query().as_str());
        out.push_str("'; Headers: '");
        for (name, value) in self.headers.iter() {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push_str("; ");
        }
        out.push('\'');

        if body.is_empty() {
            return;
        }
        match std::str::from_utf8(body) {
            Ok(body_as_str) => {
                out.push_str("Body: ");
                out.push_str(body_as_str);
            }
            Err(_) => {
                out.push_str("Body: ");
                out.push_str(body.len().to_string().as_str());
                out.push_str(" non string bytes");
            }
        }
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();
        result.push_str("PathAndQuery: '");
        result.push_str(self.url_builder.get_path_and_query().as_str());
        result.push_str("'; Headers: '");
        for (name, value) in self.headers.iter() {
            result.push_str(name);
            result.push_str(": ");
            result.push_str(value);
            result.push_str("; ");
        }
        result
    }
}

/// True when `url` is not an absolute `http(s)` URL and therefore has to be
/// resolved against the current origin (e.g. `/api/xxx`).
fn needs_origin_prefix(url: &str) -> bool {
    !(url.starts_with("http://") || url.starts_with("https://"))
}

/// Only idempotent methods are replayed by the outer retry loop, and only for
/// transport-level failures (a timeout / a `fetch` error), never for a response
/// that was actually received.
fn error_is_safe_to_retry(err: &FlUrlError, idempotent: bool) -> bool {
    if !idempotent {
        return false;
    }
    matches!(err, FlUrlError::Timeout | FlUrlError::FetchError(_))
}

fn duration_to_millis(duration: Duration) -> Option<i32> {
    let millis = duration.as_millis();
    if millis == 0 {
        return None;
    }
    Some(millis.min(i32::MAX as u128) as i32)
}
