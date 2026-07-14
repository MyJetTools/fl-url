use std::time::Duration;

use http::response::Parts;
use http_body_util::BodyExt;
use my_http_utils::UrlBuilder;

use crate::{ConnectionReturner, FlUrlError};

pub struct FlResponseAsStream {
    pub url: UrlBuilder,
    parts: Parts,
    body: http_body_util::combinators::BoxBody<bytes::Bytes, String>,
    body_read_timeout: Option<Duration>,
    // Owns the checked-out connection while the body is streaming. Returned to
    // the pool on clean end of stream; dropping the stream mid-way (or a read
    // error) disposes the connection instead.
    connection_returner: Option<Box<dyn ConnectionReturner>>,
}

impl FlResponseAsStream {
    /// Backward-compatible constructor: an unbounded, pool-less stream (no
    /// body-read timeout, no connection returned to the pool). The crate builds
    /// pooled/timed streams via [`Self::create`].
    pub fn new(url: UrlBuilder, response: my_hyper_utils::MyHttpResponse) -> Self {
        Self::create(url, response, None, None)
    }

    pub(crate) fn create(
        url: UrlBuilder,
        response: my_hyper_utils::MyHttpResponse,
        body_read_timeout: Option<Duration>,
        connection_returner: Option<Box<dyn ConnectionReturner>>,
    ) -> Self {
        let (parts, body) = response.into_parts();

        Self {
            url,
            parts,
            body,
            body_read_timeout,
            connection_returner,
        }
    }

    pub async fn get_next_chunk(&mut self) -> Result<Option<Vec<u8>>, FlUrlError> {
        let frame = match self.body_read_timeout {
            Some(timeout) => match tokio::time::timeout(timeout, self.body.frame()).await {
                Ok(frame) => frame,
                Err(_elapsed) => {
                    self.connection_returner.take();
                    return Err(FlUrlError::Timeout);
                }
            },
            None => self.body.frame().await,
        };

        let Some(frame) = frame else {
            // Clean end of stream: the body is fully consumed, the connection
            // can go back to the pool.
            self.release_connection().await;
            return Ok(None);
        };

        let frame = match frame {
            Ok(frame) => frame,
            Err(err) => {
                self.connection_returner.take();
                return Err(FlUrlError::ReadingHyperBodyError(format!("{:?}", err)));
            }
        };

        match frame.into_data() {
            Ok(value) => Ok(Some(value.to_vec())),
            // A non-data frame (e.g. HTTP/2 trailers) means no more body data
            // follows, so we treat it as a clean end of stream.
            Err(_non_data_frame) => {
                self.release_connection().await;
                Ok(None)
            }
        }
    }

    async fn release_connection(&mut self) {
        let Some(returner) = self.connection_returner.take() else {
            return;
        };

        let close_requested = self
            .parts
            .headers
            .get(hyper::header::CONNECTION)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("close"))
            .unwrap_or(false);

        let drop_by_status = crate::fl_drop_connection_scenario::should_drop_connection_by_status(
            self.parts.status.as_u16(),
        );

        if !close_requested && !drop_by_status {
            returner.return_connection().await;
        }
        // else: dropping the returner disposes the connection
    }

    pub fn get_parts(&self) -> &Parts {
        &self.parts
    }
}
