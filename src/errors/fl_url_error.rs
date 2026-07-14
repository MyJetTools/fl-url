/// The single, shared FlUrl error type. Variants that carry native-only payloads
/// (hyper / rustls / my-http-client / ssh) are compiled only for non-wasm
/// targets; the wasm backend adds [`FlUrlError::FetchError`]. It is
/// `#[non_exhaustive]`, so consumers must already carry a catch-all arm — which
/// makes the per-target variant set a non-breaking implementation detail.
#[derive(Debug)]
#[non_exhaustive]
pub enum FlUrlError {
    Timeout,
    SerializationError(serde_json::Error),
    IoError(std::io::Error),
    HttpsInvalidDomainName,
    InvalidHttp1HandShake(String),
    CanNotEstablishConnection(String),
    CanNotConvertToUtf8(std::str::Utf8Error),
    ReadingHyperBodyError(String),
    InvalidUrl(String),
    UnsupportedScheme(String),
    /// A `my_http_utils` request model failed to build (e.g. a field validator
    /// rejected its value) inside `FlUrl::execute_request`.
    RequestBuild(String),

    #[cfg(not(target_arch = "wasm32"))]
    HyperError(hyper::Error),
    #[cfg(not(target_arch = "wasm32"))]
    HttpError(hyper::http::Error),
    #[cfg(not(target_arch = "wasm32"))]
    RustTlsError(my_tls::tokio_rustls::rustls::Error),
    #[cfg(not(target_arch = "wasm32"))]
    MyHttpClientError(my_http_client::MyHttpClientError),
    #[cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]
    SshSessionError(my_ssh::SshSessionError),

    /// The browser `fetch` call (or reading the response body) failed. Carries a
    /// human-readable description of the underlying JS error / `DOMException`.
    #[cfg(target_arch = "wasm32")]
    FetchError(String),
}

impl FlUrlError {
    pub fn is_hyper_canceled(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let FlUrlError::HyperError(e) = self {
                return e.is_canceled();
            }
        }
        false
    }

    pub fn is_timeout(&self) -> bool {
        if matches!(self, FlUrlError::Timeout) {
            return true;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if matches!(
                self,
                FlUrlError::MyHttpClientError(my_http_client::MyHttpClientError::RequestTimeout(_))
            ) {
                return true;
            }
        }
        false
    }
}

impl std::fmt::Display for FlUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for FlUrlError {}

impl From<std::str::Utf8Error> for FlUrlError {
    fn from(src: std::str::Utf8Error) -> Self {
        Self::CanNotConvertToUtf8(src)
    }
}

impl From<serde_json::Error> for FlUrlError {
    fn from(src: serde_json::Error) -> Self {
        Self::SerializationError(src)
    }
}

impl From<std::io::Error> for FlUrlError {
    fn from(src: std::io::Error) -> Self {
        Self::IoError(src)
    }
}

impl From<my_http_utils::schema::client::HttpRequestBuildError> for FlUrlError {
    fn from(src: my_http_utils::schema::client::HttpRequestBuildError) -> Self {
        Self::RequestBuild(format!("{}: {}", src.field, src.reason))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<my_tls::tokio_rustls::rustls::Error> for FlUrlError {
    fn from(value: my_tls::tokio_rustls::rustls::Error) -> Self {
        Self::RustTlsError(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<my_http_client::MyHttpClientError> for FlUrlError {
    fn from(value: my_http_client::MyHttpClientError) -> Self {
        Self::MyHttpClientError(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<hyper::Error> for FlUrlError {
    fn from(src: hyper::Error) -> Self {
        Self::HyperError(src)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<hyper::http::Error> for FlUrlError {
    fn from(src: hyper::http::Error) -> Self {
        Self::HttpError(src)
    }
}

#[cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]
impl FlUrlError {
    pub fn is_ssh_session_error(&self) -> bool {
        matches!(self, FlUrlError::SshSessionError(_))
    }
}

#[cfg(all(unix, feature = "with-ssh", not(target_arch = "wasm32")))]
impl From<my_ssh::SshSessionError> for FlUrlError {
    fn from(src: my_ssh::SshSessionError) -> Self {
        Self::SshSessionError(src)
    }
}
