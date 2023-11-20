use std::collections::HashMap;

use hyper::{body::Incoming, HeaderMap, Response};

use crate::FlUrlError;

pub enum ResponseBody {
    Incoming(Option<Response<Incoming>>),
    Body {
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    },
    #[cfg(feature = "support-unix-socket")]
    UnixSocket(unix_sockets::FlUrlUnixResponse),
}

impl ResponseBody {
    pub fn as_incoming(&self) -> &Response<Incoming> {
        match &self {
            Self::Incoming(response) => response.as_ref().unwrap(),
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
            #[cfg(feature = "support-unix-socket")]
            Self::UnixSocket(_) => {
                panic!("Can not get hyper response from UnixSocket response");
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
            #[cfg(feature = "support-unix-socket")]
            Self::UnixSocket(_) => {
                panic!("Can not get hyper response from UnixSocket response");
            }
        }
    }

    pub fn get_header(&self, header: &str) -> Option<&str> {
        match self {
            Self::Incoming(response) => response
                .as_ref()
                .unwrap()
                .headers()
                .get(header)?
                .to_str()
                .unwrap()
                .into(),
            Self::Body { .. } => {
                panic!("Body is already disposed");
            }
            #[cfg(feature = "support-unix-socket")]
            Self::UnixSocket(unix_socket) => unix_socket.get_header(header),
        }
    }

    pub fn copy_headers_to_hash_map<'s>(&'s self, hash_map: &mut HashMap<&'s str, &'s str>) {
        match self {
            ResponseBody::Incoming(incoming) => {
                if let Some(incoming) = incoming {
                    for (key, value) in incoming.headers() {
                        hash_map.insert(key.as_str(), value.to_str().unwrap());
                    }
                }
            }
            ResponseBody::Body { headers, .. } => {
                for (key, value) in headers {
                    hash_map.insert(key.as_str(), value.to_str().unwrap());
                }
            }
            #[cfg(feature = "support-unix-socket")]
            ResponseBody::UnixSocket(unix_socket) => unix_socket.copy_headers_to_hashmap(hash_map),
        }
    }

    pub fn copy_headers_to_hash_map_of_string(&self, hash_map: &mut HashMap<String, String>) {
        match self {
            ResponseBody::Incoming(incoming) => {
                if let Some(incoming) = incoming {
                    for (key, value) in incoming.headers() {
                        hash_map.insert(
                            key.as_str().to_string(),
                            value.to_str().unwrap().to_string(),
                        );
                    }
                }
            }
            ResponseBody::Body { headers, .. } => {
                for (key, value) in headers {
                    hash_map.insert(
                        key.as_str().to_string(),
                        value.to_str().unwrap().to_string(),
                    );
                }
            }
            #[cfg(feature = "support-unix-socket")]
            ResponseBody::UnixSocket(unix_socket) => {
                unix_socket.copy_headers_to_hashmap_of_string(hash_map)
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
            #[cfg(feature = "support-unix-socket")]
            ResponseBody::UnixSocket(_) => {}
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
            #[cfg(feature = "support-unix-socket")]
            ResponseBody::UnixSocket(unix_socket) => Ok(unix_socket.body_as_slice()),
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
            #[cfg(feature = "support-unix-socket")]
            ResponseBody::UnixSocket(unix_socket) => Ok(unix_socket.take_body()),
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
