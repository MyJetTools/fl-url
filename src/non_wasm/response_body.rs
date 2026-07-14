use std::collections::HashMap;
use std::time::Duration;

use http_body_util::BodyExt;
use hyper::HeaderMap;
use my_http_client::HyperResponse;

use crate::{FlUrlError, FlUrlReadingHeaderError};

pub enum ResponseBody {
    Hyper(Option<my_hyper_utils::MyHttpResponse>),
    Body {
        status_code: http::StatusCode,
        version: http::Version,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    },
}

impl ResponseBody {
    pub fn as_hyper_response(&self) -> &my_hyper_utils::MyHttpResponse {
        match &self {
            Self::Hyper(response) => response.as_ref().unwrap(),
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn into_hyper_response(self) -> my_hyper_utils::MyHttpResponse {
        match self {
            Self::Hyper(response) => {
                let response = response.unwrap();
                response
            }
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn get_header(&self, header: &str) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        let headers = match self {
            Self::Hyper(response) => response.as_ref().unwrap().headers(),
            Self::Body { headers, .. } => headers,
        };

        let result = headers.get(header);

        if result.is_none() {
            return Ok(None);
        }

        let value = result.unwrap().to_str()?;

        Ok(Some(value))
    }

    pub fn get_header_case_insensitive(
        &self,
        header: &str,
    ) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        let headers = match self {
            Self::Hyper(response) => response.as_ref().unwrap().headers(),
            Self::Body { headers, .. } => headers,
        };

        for (name, value) in headers.iter() {
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
        match self {
            ResponseBody::Hyper(incoming) => {
                if let Some(incoming) = incoming {
                    for (key, value) in incoming.headers() {
                        if let Ok(value) = value.to_str() {
                            hash_map.insert(key.as_str(), Some(value));
                        }
                    }
                }
            }
            ResponseBody::Body { headers, .. } => {
                for (key, value) in headers {
                    if let Ok(value) = value.to_str() {
                        hash_map.insert(key.as_str(), Some(value));
                    }
                }
            }
        }
    }

    pub fn copy_headers_to_hash_map_of_string(
        &self,
        hash_map: &mut HashMap<String, Option<String>>,
    ) {
        match self {
            ResponseBody::Hyper(incoming) => {
                if let Some(incoming) = incoming {
                    for (key, value) in incoming.headers() {
                        hash_map.insert(
                            key.as_str().to_string(),
                            if let Ok(value) = value.to_str() {
                                Some(value.to_string())
                            } else {
                                None
                            },
                        );
                    }
                }
            }
            ResponseBody::Body { headers, .. } => {
                for (key, value) in headers {
                    hash_map.insert(
                        key.as_str().to_string(),
                        if let Ok(value) = value.to_str() {
                            Some(value.to_string())
                        } else {
                            None
                        },
                    );
                }
            }
        }
    }

    pub(crate) async fn convert_to_slice_if_needed(
        &mut self,
        body_read_timeout: Option<Duration>,
    ) -> Result<(), FlUrlError> {
        match self {
            Self::Hyper(response) => {
                let response = response.take().unwrap();

                let status_code = response.status();
                let version = response.version();

                let (parts, incoming) = response.into_parts();

                // Written BEFORE the await: if the read future is dropped mid-way
                // (cancellation) or fails, the enum stays in a valid state —
                // headers remain reachable and body reads return an error
                // instead of panicking on a taken-out `Hyper(None)`.
                *self = Self::Body {
                    status_code,
                    version,
                    headers: parts.headers,
                    body: None,
                };

                let read_future = my_hyper_utils::box_body_to_vec(incoming, |err| {
                    FlUrlError::ReadingHyperBodyError(err)
                });

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
            Self::Body { .. } => Ok(()),
        }
    }

    /// Decodes a gzip-encoded loaded body in place and updates the
    /// `Content-Encoding` / `Content-Length` headers to match. A pure data-level
    /// transform: it runs only after the body has been fully read off the wire
    /// and the connection settled, so a decode error does not affect connection
    /// reuse.
    pub(crate) fn decode_gzip_if_needed(&mut self) -> Result<(), FlUrlError> {
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

        let decoded = decompress_gzip_body(compressed.as_slice())?;

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

    pub async fn convert_body_and_get_as_slice(&mut self) -> Result<&[u8], FlUrlError> {
        self.convert_to_slice_if_needed(None).await?;
        self.get_loaded_body_as_slice()
    }

    pub async fn convert_body_and_receive_it(&mut self) -> Result<Vec<u8>, FlUrlError> {
        self.convert_to_slice_if_needed(None).await?;
        self.take_loaded_body()
    }
    pub fn into_http_body(self) -> Result<HyperResponse, FlUrlError> {
        match self {
            ResponseBody::Hyper(mut response) => {
                let result = response.take().unwrap();
                Ok(result)
            }
            ResponseBody::Body {
                status_code,
                version,
                headers,
                body,
            } => {
                let result = my_hyper_utils::compile_full_body(
                    status_code,
                    version,
                    headers,
                    body.unwrap_or_default(),
                    |builder, full_body| {
                        builder
                            .body(full_body.map_err(|itm| itm.to_string()).boxed())
                            .unwrap()
                    },
                );

                Ok(result)
            }
        }
    }

    pub async fn into_http_full_body(
        self,
    ) -> Result<http::Response<http_body_util::Full<hyper::body::Bytes>>, FlUrlError> {
        match self {
            ResponseBody::Hyper(response) => {
                let response = response.unwrap();
                let status_code = response.status();
                let version = response.version();
                let (parts, body) = response.into_parts();

                let body = my_hyper_utils::box_body_to_vec(body, |err| {
                    FlUrlError::ReadingHyperBodyError(err)
                })
                .await?;

                let result = my_hyper_utils::compile_full_body(
                    status_code,
                    version,
                    parts.headers,
                    body,
                    |builder, full_body| builder.body(full_body).unwrap(),
                );

                Ok(result)
            }
            ResponseBody::Body {
                status_code,
                version,
                headers,
                body,
            } => {
                let result = my_hyper_utils::compile_full_body(
                    status_code,
                    version,
                    headers,
                    body.unwrap_or_default(),
                    |builder, full_body| builder.body(full_body).unwrap(),
                );

                Ok(result)
            }
        }
    }
}

fn decompress_gzip_body(data: &[u8]) -> Result<Vec<u8>, FlUrlError> {
    use std::io::Read;

    // MultiGzDecoder (not GzDecoder) so concatenated gzip members — a valid,
    // spec-allowed encoding some servers emit — are all decoded, not just the
    // first.
    let mut decoder = flate2::read::MultiGzDecoder::new(data);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result).map_err(|err| {
        FlUrlError::ReadingHyperBodyError(format!("Failed to decompress gzip body: {}", err))
    })?;
    Ok(result)
}

/*
fn compile_full_body<TResult>(
    status_code: http::StatusCode,
    version: http::Version,
    headers: HeaderMap,
    body: Vec<u8>,
    compiler: impl Fn(
        http::response::Builder,
        http_body_util::Full<hyper::body::Bytes>,
    ) -> http::Response<TResult>,
) -> http::Response<TResult> {
    let mut builder = http::response::Builder::new()
        .status(status_code)
        .version(version);

    let mut has_content_len = false;

    for header in headers {
        if let Some(header_name) = header.0 {
            if header_name
                .as_str()
                .eq_ignore_ascii_case(CONTENT_LENGTH.as_str())
            {
                has_content_len = true;
            }

            if header_name
                .as_str()
                .eq_ignore_ascii_case(TRANSFER_ENCODING.as_str())
            {
                continue;
            }

            builder = builder.header(header_name, header.1);
        }
    }

    if body.len() > 0 {
        if !has_content_len {
            builder = builder.header(CONTENT_LENGTH, body.len());
        }
    }

    let full_body = http_body_util::Full::new(hyper::body::Bytes::from(body));

    compiler(builder, full_body)
}

async fn body_to_vec(
    body: http_body_util::combinators::BoxBody<bytes::Bytes, String>,
) -> Result<Vec<u8>, FlUrlError> {
    let collected = body.collect().await;

    match collected {
        Ok(bytes) => {
            let bytes = bytes.to_bytes();
            Ok(bytes.into())
        }
        Err(err) => {
            let err = FlUrlError::ReadingHyperBodyError(err);
            Err(err)
        }
    }
}
 */
