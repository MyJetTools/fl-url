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
