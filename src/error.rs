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
