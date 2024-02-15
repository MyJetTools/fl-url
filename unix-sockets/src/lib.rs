mod unix_socket_client;
use std::collections::HashMap;

use hyper::{header::ToStrError, HeaderMap};
pub use unix_socket_client::*;
mod url_builder_owned;
pub use url_builder_owned::*;

#[derive(Debug)]
pub enum FlUrlUnixSocketError {
    HyperError(String),
}

pub struct FlUrlUnixResponse {
    pub body: Option<Vec<u8>>,
    pub headers: HeaderMap,
    pub status_code: u16,
}

impl FlUrlUnixResponse {
    pub fn new(status_code: u16, headers: HeaderMap, body: Vec<u8>) -> Self {
        Self {
            body: Some(body),
            status_code,
            headers,
        }
    }

    pub fn get_header(&self, name: &str) -> Result<Option<&str>, ToStrError> {
        let result = self.headers.get(name);

        match result {
            Some(result) => {
                let result = result.to_str()?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    pub fn get_header_case_insensitive(&self, name: &str) -> Result<Option<&str>, ToStrError> {
        for (header_name, value) in self.headers.iter() {
            if rust_extensions::str_utils::compare_strings_case_insensitive(
                name,
                header_name.as_str(),
            ) {
                let result = value.to_str()?;
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    pub fn copy_headers_to_hashmap_of_string(&self, result: &mut HashMap<String, Option<String>>) {
        for (key, value) in &self.headers {
            if let Ok(value) = value.to_str() {
                result.insert(key.to_string(), Some(value.to_string()));
            }
        }
    }

    pub fn copy_headers_to_hashmap<'s>(&'s self, result: &mut HashMap<&'s str, Option<&'s str>>) {
        for (key, value) in &self.headers {
            if let Ok(value) = value.to_str() {
                result.insert(key.as_str(), Some(value));
            }
        }
    }

    pub fn body_as_slice(&self) -> &[u8] {
        match &self.body {
            Some(res) => res,
            None => panic!("Unix Response body is already disposed"),
        }
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        match self.body.take() {
            Some(res) => res,
            None => panic!("Unix Response body is already disposed"),
        }
    }
}
