use hyper_unix_connector::UnixClient;

use crate::{FlUrlError, FlUrlResponse, UrlBuilder, UrlBuilderOwned};

pub struct UnixSocketClient {
    _client: UnixClient,
    _url: UrlBuilderOwned,
}

impl UnixSocketClient {
    pub fn new(url: UrlBuilder) -> Self {
        Self {
            _client: UnixClient,
            _url: url.into_builder_owned(),
        }
    }

    pub async fn execute_request(&self) -> Result<FlUrlResponse, FlUrlError> {
        panic!("Unix sockets are not implemented yet");
    }
}

/*
#[cfg(feature = "support-unix-socket")]
async fn execute_unix_socket(self) -> Result<FlUrlResponse, FlUrlError> {
    use hyper_unix_connector::UnixClient;
    let client: hyper::Client<UnixClient, hyper::Body> = hyper::Client::builder().build(UnixClient);

    let url = self.url.into_builder_owned();

    let addr: hyper::Uri = hyper_unix_connector::Uri::new(
        self.url.get_scheme_and_host().as_str(),
        self.url.get_path_and_query().as_str(),
    )
    .into();

    let result = client.get(addr).await;

    match result {
        Ok(result) => {
            return Ok(FlUrlResponse::new(url, result));
        }
        Err(err) => {
            return Err(FlUrlError::HyperError(err));
        }
    }
}
 */
