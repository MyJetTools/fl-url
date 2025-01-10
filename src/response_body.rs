use std::collections::HashMap;

use http_body_util::BodyExt;
use hyper::HeaderMap;
use my_http_client::HyperResponse;

use crate::{FlUrlError, FlUrlReadingHeaderError};

pub enum ResponseBody {
    Hyper(Option<my_http_client::HyperResponse>),
    Body {
        status_code: http::StatusCode,
        version: http::Version,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    },
}

impl ResponseBody {
    pub fn as_hyper_response(&self) -> &my_http_client::HyperResponse {
        match &self {
            Self::Hyper(response) => response.as_ref().unwrap(),
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn into_hyper_response(self) -> my_http_client::HyperResponse {
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
        match self {
            Self::Hyper(response) => {
                let result = response.as_ref().unwrap().headers().get(header);

                if result.is_none() {
                    return Ok(None);
                }

                let value = result.unwrap().to_str()?;

                return Ok(Some(value));
            }
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn get_header_case_insensitive(
        &self,
        header: &str,
    ) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        match self {
            Self::Hyper(response) => {
                for (name, value) in response.as_ref().unwrap().headers().iter() {
                    if rust_extensions::str_utils::compare_strings_case_insensitive(
                        name.as_str(),
                        header,
                    ) {
                        let value = value.to_str()?;
                        return Ok(Some(value));
                    }
                }

                return Ok(None);
            }
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
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

    async fn convert_to_slice_if_needed(&mut self) -> Result<(), FlUrlError> {
        match self {
            Self::Hyper(response) => {
                let response = response.take().unwrap();

                let status_code = response.status();
                let version = response.version();

                let (parts, incoming) = response.into_parts();

                let body = my_hyper_utils::box_body_to_vec(incoming, |err| {
                    FlUrlError::ReadingHyperBodyError(err)
                })
                .await?;
                *self = Self::Body {
                    status_code,
                    version,
                    headers: parts.headers,
                    body: Some(body),
                }
            }
            Self::Body { .. } => {}
        }

        Ok(())
    }

    pub async fn convert_body_and_get_as_slice(&mut self) -> Result<&[u8], FlUrlError> {
        self.convert_to_slice_if_needed().await?;

        match self {
            ResponseBody::Hyper(_) => {
                panic!("Should not be here")
            }
            ResponseBody::Body { body, .. } => match body {
                Some(body) => Ok(body.as_slice()),
                None => panic!("Body is already disposed"),
            },
        }
    }

    pub async fn convert_body_and_receive_it(&mut self) -> Result<Vec<u8>, FlUrlError> {
        self.convert_to_slice_if_needed().await?;

        match self {
            ResponseBody::Hyper(_) => {
                panic!("Should not be here")
            }
            ResponseBody::Body { body, .. } => match body.take() {
                Some(body) => Ok(body),
                None => panic!("Body is already disposed"),
            },
        }
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
