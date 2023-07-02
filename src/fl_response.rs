use std::collections::HashMap;

use hyper::{Body, Response};

use crate::{FlUrlError, UrlUriBuilder};
pub struct FlUrlResponse {
    pub url: UrlUriBuilder,
    status_code: u16,
    pub response: Option<Response<Body>>,
    body: Option<Vec<u8>>,
}

impl FlUrlResponse {
    pub fn new(url: UrlUriBuilder, response: Response<Body>) -> Self {
        Self {
            status_code: response.status().as_u16(),
            response: Some(response),
            body: None,
            url,
        }
    }

    fn get_response(&self) -> &Response<Body> {
        match &self.response {
            Some(response) => response,
            None => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn read_header(&self, name: &str) -> Option<&str> {
        self.get_response()
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap())
    }

    pub fn get_headers(&self) -> HashMap<&str, &str> {
        let mut result = HashMap::new();

        let headers = self.get_response().headers();

        for (header_name, header_val) in headers.into_iter() {
            let key = header_name.as_str();

            let value = std::str::from_utf8(header_val.as_bytes()).unwrap();
            result.insert(key, value);
        }

        result
    }

    pub fn fill_headers_to_hashmap(&self, dest: &mut HashMap<String, String>) {
        let headers = self.get_response().headers();

        for (header_name, header_val) in headers.into_iter() {
            let key = header_name.as_str();

            let value = std::str::from_utf8(header_val.as_bytes()).unwrap();
            dest.insert(key.to_string(), value.to_string());
        }
    }

    async fn init_body(&mut self) -> Result<(), FlUrlError> {
        if self.body.is_some() {
            return Ok(());
        }

        let mut result = None;
        std::mem::swap(&mut self.response, &mut result);

        if result.is_none() {
            panic!("Body can not be received for a second time");
        }

        let body = result.unwrap().into_body();
        let full_body = hyper::body::to_bytes(body).await?;

        let result: Vec<u8> = full_body.into_iter().collect();

        self.body = Some(result);

        Ok(())
    }

    pub async fn get_body(&mut self) -> Result<&[u8], FlUrlError> {
        self.init_body().await?;

        match &self.body {
            Some(result) => Ok(result),
            None => {
                panic!("Body is already disposed");
            }
        }
    }

    pub async fn receive_body(mut self) -> Result<Vec<u8>, FlUrlError> {
        self.init_body().await?;
        match self.body {
            Some(result) => Ok(result),
            None => {
                panic!("Body is already disposed");
            }
        }
    }

    pub fn get_status_code(&self) -> u16 {
        self.status_code
    }
}
