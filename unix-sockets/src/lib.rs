mod unix_socket_client;
use std::collections::HashMap;

use hyper::HeaderMap;
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

    pub fn get_header(&self, name: &str) -> Option<&str> {
        let result = self.headers.get(name)?;
        result.to_str().ok()
    }

    pub fn copy_headers_to_hashmap<'s>(&'s self, result: &mut HashMap<&'s str, &'s str>) {
        for (key, value) in &self.headers {
            result.insert(key.as_str(), value.to_str().unwrap());
        }
    }

    pub fn copy_headers_to_hashmap_of_string(&self, result: &mut HashMap<String, String>) {
        for (key, value) in &self.headers {
            result.insert(
                key.as_str().to_string(),
                value.to_str().unwrap().to_string(),
            );
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
