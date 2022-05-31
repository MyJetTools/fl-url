pub enum FlUrlError {
    HyperError(hyper::Error),
    Timeout,
}

impl From<hyper::Error> for FlUrlError {
    fn from(src: hyper::Error) -> Self {
        Self::HyperError(src)
    }
}
