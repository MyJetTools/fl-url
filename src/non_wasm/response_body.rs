use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::header::{HeaderValue, CONTENT_LENGTH, TRANSFER_ENCODING};
use hyper::HeaderMap;
use my_http_client::HyperResponse;

use crate::{FlUrlError, FlUrlReadingHeaderError};

const BODY_NOT_AVAILABLE: &str =
    "Response body is not available (already consumed or failed to read)";

pub enum ResponseBody {
    Hyper(my_hyper_utils::MyHttpResponse),
    Body {
        status_code: http::StatusCode,
        version: http::Version,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    },
}

impl ResponseBody {
    /// `None` once the body has been read into memory: there is no hyper response to
    /// lend any more. [`Self::into_hyper_response`] makes one of what was read.
    pub fn as_hyper_response(&self) -> Option<&my_hyper_utils::MyHttpResponse> {
        match self {
            Self::Hyper(response) => Some(response),
            Self::Body { .. } => None,
        }
    }

    /// The response as hyper's. A body read into memory already is made into one
    /// again; a body whose read failed comes back as one whose first frame fails with
    /// the reason.
    pub fn into_hyper_response(self) -> my_hyper_utils::MyHttpResponse {
        match self {
            Self::Hyper(response) => response,
            Self::Body {
                status_code,
                version,
                headers,
                body: Some(body),
            } => full_body_response(status_code, version, headers, body)
                .map(|body| body.map_err(|err| err.to_string()).boxed()),
            Self::Body {
                status_code,
                version,
                headers,
                body: None,
            } => with_head(
                status_code,
                version,
                headers,
                UnavailableBody(Some(BODY_NOT_AVAILABLE.to_string())).boxed(),
            ),
        }
    }

    fn headers(&self) -> &HeaderMap {
        match self {
            Self::Hyper(response) => response.headers(),
            Self::Body { headers, .. } => headers,
        }
    }

    /// The `Content-Length` of the response, when it carries one that reads as a number
    /// and frames the body (see [`content_length`]).
    pub(crate) fn content_length(&self) -> Option<u64> {
        content_length(self.headers())
    }

    pub fn get_header(&self, header: &str) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        match self.headers().get(header) {
            Some(value) => Ok(Some(value.to_str()?)),
            None => Ok(None),
        }
    }

    pub fn get_header_case_insensitive(
        &self,
        header: &str,
    ) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        for (name, value) in self.headers().iter() {
            if rust_extensions::str_utils::compare_strings_case_insensitive(name.as_str(), header) {
                let value = value.to_str()?;
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    pub fn copy_headers_to_hash_map<'s>(
        &'s self,
        hash_map: &mut HashMap<&'s str, Option<&'s str>>,
    ) {
        for (key, value) in self.headers() {
            if let Ok(value) = value.to_str() {
                hash_map.insert(key.as_str(), Some(value));
            }
        }
    }

    pub fn copy_headers_to_hash_map_of_string(
        &self,
        hash_map: &mut HashMap<String, Option<String>>,
    ) {
        for (key, value) in self.headers() {
            hash_map.insert(
                key.as_str().to_string(),
                value.to_str().ok().map(|value| value.to_string()),
            );
        }
    }

    /// `announced_size` is what the response said its body is - `None` when it said
    /// nothing, or when what it said is not about a body it carries (a HEAD response).
    pub(crate) async fn convert_to_slice_if_needed(
        &mut self,
        body_read_timeout: Option<Duration>,
        max_body_size: usize,
        announced_size: Option<u64>,
    ) -> Result<(), FlUrlError> {
        // The placeholder lives only until the line that replaces it below, with no
        // await in between, so no one ever sees it.
        let placeholder = Self::Body {
            status_code: http::StatusCode::OK,
            version: http::Version::HTTP_11,
            headers: HeaderMap::new(),
            body: None,
        };

        match std::mem::replace(self, placeholder) {
            Self::Hyper(response) => {
                let status_code = response.status();
                let version = response.version();

                let (parts, incoming) = response.into_parts();

                // Written BEFORE the await: if the read future is dropped mid-way
                // (cancellation) or fails, the enum stays in a valid state —
                // headers remain reachable and body reads return an error.
                *self = Self::Body {
                    status_code,
                    version,
                    headers: parts.headers,
                    body: None,
                };

                // A body that announces it is too large is refused before a byte of
                // it is read; one that does not is cut off once it grows past the
                // limit. Either way body stays None and the connection is disposed.
                if announced_size.is_some_and(|size| size > max_body_size as u64) {
                    return Err(body_too_large(max_body_size));
                }

                let read_future = read_body_limited(incoming, max_body_size);

                let body_result = match body_read_timeout {
                    Some(timeout) => match tokio::time::timeout(timeout, read_future).await {
                        Ok(result) => result,
                        Err(_elapsed) => Err(FlUrlError::Timeout),
                    },
                    None => read_future.await,
                };

                // A read error/timeout leaves body: None -> the caller disposes
                // the connection. A successful read stores the raw body; gzip
                // decoding is a SEPARATE, body-level step (decode_gzip_if_needed)
                // so a decode failure never disposes an already-drained socket.
                let body = body_result?;

                if let Self::Body { body: dest, .. } = self {
                    *dest = Some(body);
                }

                Ok(())
            }
            materialized => {
                *self = materialized;
                Ok(())
            }
        }
    }

    /// Decodes a gzip-encoded loaded body in place and updates the
    /// `Content-Encoding` / `Content-Length` headers to match. A pure data-level
    /// transform: it runs only after the body has been fully read off the wire
    /// and the connection settled, so a decode error does not affect connection
    /// reuse.
    pub(crate) fn decode_gzip_if_needed(&mut self, max_body_size: usize) -> Result<(), FlUrlError> {
        let Self::Body { headers, body, .. } = self else {
            return Ok(());
        };

        let is_gzip = headers
            .get(hyper::header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("gzip"))
            .unwrap_or(false);

        if !is_gzip {
            return Ok(());
        }

        let Some(compressed) = body.as_ref() else {
            return Ok(());
        };

        if compressed.is_empty() {
            return Ok(());
        }

        let decoded = decompress_gzip_body(compressed.as_slice(), max_body_size)?;

        // The stored headers must describe the body we actually hold, not the
        // compressed wire form.
        headers.remove(hyper::header::CONTENT_ENCODING);
        if headers.contains_key(hyper::header::CONTENT_LENGTH) {
            headers.insert(
                hyper::header::CONTENT_LENGTH,
                hyper::header::HeaderValue::from(decoded.len()),
            );
        }
        *body = Some(decoded);

        Ok(())
    }

    /// `true` only when the body has actually been read into memory. The
    /// materialized variant with `body: None` (left behind by a cancelled or
    /// failed read) does NOT count — the socket still carries unread bytes.
    pub(crate) fn has_loaded_body(&self) -> bool {
        matches!(self, ResponseBody::Body { body: Some(_), .. })
    }

    pub(crate) fn get_loaded_body_as_slice(&self) -> Result<&[u8], FlUrlError> {
        match self {
            ResponseBody::Hyper(_) => Err(FlUrlError::ReadingHyperBodyError(
                "Response body is not loaded yet".to_string(),
            )),
            ResponseBody::Body { body, .. } => match body {
                Some(body) => Ok(body.as_slice()),
                None => Err(FlUrlError::ReadingHyperBodyError(
                    "Response body is not available (already consumed or failed to read)"
                        .to_string(),
                )),
            },
        }
    }

    pub(crate) fn take_loaded_body(&mut self) -> Result<Vec<u8>, FlUrlError> {
        match self {
            ResponseBody::Hyper(_) => Err(FlUrlError::ReadingHyperBodyError(
                "Response body is not loaded yet".to_string(),
            )),
            ResponseBody::Body { body, .. } => match body.take() {
                Some(body) => Ok(body),
                None => Err(FlUrlError::ReadingHyperBodyError(
                    "Response body is not available (already consumed or failed to read)"
                        .to_string(),
                )),
            },
        }
    }

    // These helpers have no size limit to be given, so they keep reading a body of any
    // size, as they always did; the limit is a setting of FlUrl's buffered reads.
    pub async fn convert_body_and_get_as_slice(&mut self) -> Result<&[u8], FlUrlError> {
        self.convert_to_slice_if_needed(None, usize::MAX, None).await?;
        self.get_loaded_body_as_slice()
    }

    pub async fn convert_body_and_receive_it(&mut self) -> Result<Vec<u8>, FlUrlError> {
        self.convert_to_slice_if_needed(None, usize::MAX, None).await?;
        self.take_loaded_body()
    }
    pub fn into_http_body(self) -> Result<HyperResponse, FlUrlError> {
        match self {
            ResponseBody::Hyper(response) => Ok(response),
            ResponseBody::Body {
                status_code,
                version,
                headers,
                body,
            } => {
                let body = body.ok_or_else(body_not_available)?;

                Ok(full_body_response(status_code, version, headers, body)
                    .map(|body| body.map_err(|err| err.to_string()).boxed()))
            }
        }
    }

    pub async fn into_http_full_body(
        self,
    ) -> Result<http::Response<http_body_util::Full<hyper::body::Bytes>>, FlUrlError> {
        let (status_code, version, headers, body) = match self {
            ResponseBody::Hyper(response) => {
                let status_code = response.status();
                let version = response.version();
                let (parts, body) = response.into_parts();

                let body = read_body_limited(body, usize::MAX).await?;

                (status_code, version, parts.headers, body)
            }
            ResponseBody::Body {
                status_code,
                version,
                headers,
                body,
            } => (
                status_code,
                version,
                headers,
                body.ok_or_else(body_not_available)?,
            ),
        };

        Ok(full_body_response(status_code, version, headers, body))
    }
}

fn body_too_large(limit: usize) -> FlUrlError {
    FlUrlError::ResponseBodyTooLarge { limit }
}

/// The `Content-Length` the response announced, when it carries one that reads as a
/// number. Next to a `Transfer-Encoding` it is not what frames the body (RFC 9112
/// §6.3), so then it announces nothing. Read as `u64` so that a size past `usize` on
/// a 32-bit target still counts as too large rather than as no size at all.
fn content_length(headers: &HeaderMap) -> Option<u64> {
    if headers.contains_key(TRANSFER_ENCODING) {
        return None;
    }
    headers.get(CONTENT_LENGTH)?.to_str().ok()?.trim().parse().ok()
}

/// Reads the body into memory, failing as soon as it grows past `max_body_size`
/// instead of buffering whatever the server sends.
async fn read_body_limited(
    mut body: http_body_util::combinators::BoxBody<Bytes, String>,
    max_body_size: usize,
) -> Result<Vec<u8>, FlUrlError> {
    let mut result = Vec::new();

    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(FlUrlError::ReadingHyperBodyError)?;

        let Ok(data) = frame.into_data() else {
            continue;
        };

        if data.len() > max_body_size - result.len() {
            return Err(body_too_large(max_body_size));
        }

        if result.is_empty() {
            // A body that arrives as one frame is taken over without a copy.
            result = Vec::from(data);
        } else {
            result.extend_from_slice(&data);
        }
    }

    Ok(result)
}

fn body_not_available() -> FlUrlError {
    FlUrlError::ReadingHyperBodyError(BODY_NOT_AVAILABLE.to_string())
}

/// A response made of a body that is in memory as a whole. The head is the one that
/// came over the wire, framed by the length now: no `Transfer-Encoding`, and a
/// `Content-Length` unless it carries one already — a HEAD response keeps the length
/// it was given, and a decoded gzip body has had its own put in by the decoder.
fn full_body_response(
    status_code: http::StatusCode,
    version: http::Version,
    mut headers: HeaderMap,
    body: Vec<u8>,
) -> http::Response<http_body_util::Full<Bytes>> {
    headers.remove(TRANSFER_ENCODING);

    if !body.is_empty() && !headers.contains_key(CONTENT_LENGTH) {
        headers.insert(CONTENT_LENGTH, HeaderValue::from(body.len()));
    }

    with_head(
        status_code,
        version,
        headers,
        http_body_util::Full::new(Bytes::from(body)),
    )
}

/// Puts a head together with a body. A builder would take the same, but it reports
/// what it did not take through a `Result` — and none of these can be refused.
fn with_head<TBody>(
    status_code: http::StatusCode,
    version: http::Version,
    headers: HeaderMap,
    body: TBody,
) -> http::Response<TBody> {
    let mut response = http::Response::new(body);
    *response.status_mut() = status_code;
    *response.version_mut() = version;
    *response.headers_mut() = headers;
    response
}

/// What a body that could not be read turns into when its response is handed on: it
/// has nothing to give but the reason, and its first frame fails with it.
struct UnavailableBody(Option<String>);

impl hyper::body::Body for UnavailableBody {
    type Data = Bytes;
    type Error = String;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, String>>> {
        Poll::Ready(self.0.take().map(Err))
    }
}

fn decompress_gzip_body(data: &[u8], max_body_size: usize) -> Result<Vec<u8>, FlUrlError> {
    use std::io::Read;

    // MultiGzDecoder (not GzDecoder) so concatenated gzip members — a valid,
    // spec-allowed encoding some servers emit — are all decoded, not just the
    // first.
    let decoder = flate2::read::MultiGzDecoder::new(data);
    // The decoded body is held to the same limit: a few kilobytes of gzip can
    // inflate into gigabytes. One byte past the limit is enough to tell.
    let mut decoder = decoder.take((max_body_size as u64).saturating_add(1));
    let mut result = Vec::new();
    decoder.read_to_end(&mut result).map_err(|err| {
        FlUrlError::ReadingHyperBodyError(format!("Failed to decompress gzip body: {}", err))
    })?;

    if result.len() > max_body_size {
        return Err(body_too_large(max_body_size));
    }

    Ok(result)
}
