use std::collections::HashMap;

use hyper::{body::Incoming, Response};
use serde::de::DeserializeOwned;

use crate::{FlUrlError, ResponseBody, UrlBuilderOwned};
pub struct FlUrlResponse {
    pub url: UrlBuilderOwned,
    status_code: u16,
    response: ResponseBody,
}

impl FlUrlResponse {
    pub fn new(url: UrlBuilderOwned, response: Response<Incoming>) -> Self {
        Self {
            status_code: response.status().as_u16(),
            response: ResponseBody::Incoming(response.into()),
            url,
        }
    }

    pub fn into_hyper_response(self) -> Response<Incoming> {
        self.response.into_hyper_response()
    }

    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.response.get_header(name)
    }

    pub fn get_headers(&self) -> HashMap<&str, &str> {
        let mut result = HashMap::new();

        self.response.copy_headers_to_hash_map(&mut result);

        result
    }

    pub fn fill_headers_to_hashmap(&self, dest: &mut HashMap<String, String>) {
        self.response.copy_headers_to_hash_map_of_string(dest);
    }

    pub async fn get_body_as_slice(&mut self) -> Result<&[u8], FlUrlError> {
        self.response.convert_body_and_get_as_slice().await
    }

    pub async fn get_json<TResponse: DeserializeOwned>(&mut self) -> Result<TResponse, FlUrlError> {
        let body = self.response.convert_body_and_get_as_slice().await?;
        let result = serde_json::from_slice(body)?;
        Ok(result)
    }

    pub async fn receive_body(mut self) -> Result<Vec<u8>, FlUrlError> {
        self.response.convert_body_and_receive_it().await
    }

    pub async fn body_as_str(&mut self) -> Result<&str, FlUrlError> {
        let bytes = self.response.convert_body_and_get_as_slice().await?;
        Ok(std::str::from_utf8(bytes).unwrap())
    }

    pub fn get_status_code(&self) -> u16 {
        self.status_code
    }
}
