#[derive(Debug)]
pub enum FlUrlReadingHeaderError {
    CanNotConvertToUtf8(hyper::header::ToStrError),
    CanNotConvertUnixSocketHeaderToUtf8(String),
}

impl From<hyper::header::ToStrError> for FlUrlReadingHeaderError {
    fn from(src: hyper::header::ToStrError) -> Self {
        Self::CanNotConvertToUtf8(src)
    }
}

impl std::fmt::Display for FlUrlReadingHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for FlUrlReadingHeaderError {}
