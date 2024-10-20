#[derive(Debug)]
pub enum FlUrlError {
    HyperError(hyper::Error),
    Timeout,
    SerializationError(serde_json::Error),
    IoError(std::io::Error),
    HttpError(hyper::http::Error),
    HttpsInvalidDomainName,
    ConnectionIsDead,
    InvalidHttp1HandShake(String),
    CanNotEstablishConnection(String),
    ClientCertificateError(tokio_rustls::rustls::Error),
    CanNotConvertToUtf8(std::str::Utf8Error),

    #[cfg(feature = "with-ssh")]
    SshSessionError(my_ssh::SshSessionError),
}

impl FlUrlError {
    pub fn is_hyper_canceled(&self) -> bool {
        match self {
            FlUrlError::HyperError(e) => e.is_canceled(),
            _ => false,
        }
    }
}

#[cfg(feature = "with-ssh")]
impl FlUrlError {
    pub fn is_ssh_session_error(&self) -> bool {
        match self {
            FlUrlError::SshSessionError(_) => true,
            _ => false,
        }
    }
}

#[cfg(feature = "with-ssh")]
impl From<my_ssh::SshSessionError> for FlUrlError {
    fn from(src: my_ssh::SshSessionError) -> Self {
        Self::SshSessionError(src)
    }
}

impl From<std::str::Utf8Error> for FlUrlError {
    fn from(src: std::str::Utf8Error) -> Self {
        Self::CanNotConvertToUtf8(src)
    }
}

impl From<hyper::Error> for FlUrlError {
    fn from(src: hyper::Error) -> Self {
        Self::HyperError(src)
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

impl From<hyper::http::Error> for FlUrlError {
    fn from(src: hyper::http::Error) -> Self {
        Self::HttpError(src)
    }
}
