use http::response::Parts;
use http_body_util::BodyExt;
use url_utils::UrlBuilder;

use crate::FlUrlError;

pub struct FlResponseAsStream {
    pub url: UrlBuilder,
    parts: Parts,
    body: http_body_util::combinators::BoxBody<bytes::Bytes, String>,
}

impl FlResponseAsStream {
    pub fn new(url: UrlBuilder, response: my_hyper_utils::MyHttpResponse) -> Self {
        let (parts, body) = response.into_parts();

        Self { url, parts, body }
    }
    pub async fn get_next_chunk(&mut self) -> Result<Option<Vec<u8>>, FlUrlError> {
        let Some(frame) = self.body.frame().await else {
            return Ok(None);
        };

        let frame = frame.unwrap();

        let data = frame.into_data();

        match data {
            Ok(value) => {
                let result = value.to_vec();
                Ok(Some(result))
            }
            Err(err) => {
                return Err(FlUrlError::ReadingHyperBodyError(format!("{:?}", err)));
            }
        }
    }
    pub fn get_parts(&self) -> &Parts {
        &self.parts
    }
}
