use std::collections::HashMap;

use hyper::{body::Incoming, HeaderMap, Response};

use crate::{FlUrlError, FlUrlReadingHeaderError};

pub enum ResponseBody {
    Incoming(Option<Response<Incoming>>),
    Body {
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    },
}

impl ResponseBody {
    pub fn as_incoming(&self) -> &Response<Incoming> {
        match &self {
            Self::Incoming(response) => response.as_ref().unwrap(),
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn into_hyper_response(self) -> Response<Incoming> {
        match self {
            Self::Incoming(response) => {
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
            Self::Incoming(response) => {
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
            Self::Incoming(response) => {
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
            ResponseBody::Incoming(incoming) => {
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
            ResponseBody::Incoming(incoming) => {
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
            Self::Incoming(response) => {
                let response = response.take().unwrap();

                let (parts, incoming) = response.into_parts();

                let body = read_bytes(incoming).await?;
                *self = Self::Body {
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
            ResponseBody::Incoming(_) => {
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
            ResponseBody::Incoming(_) => {
                panic!("Should not be here")
            }
            ResponseBody::Body { body, .. } => match body.take() {
                Some(body) => Ok(body),
                None => panic!("Body is already disposed"),
            },
        }
    }
}

async fn read_bytes(
    incoming: impl hyper::body::Body<Data = hyper::body::Bytes, Error = hyper::Error>,
) -> Result<Vec<u8>, FlUrlError> {
    use http_body_util::BodyExt;

    let collected = incoming.collect().await?;
    let bytes = collected.to_bytes();
    Ok(bytes.into())
}
