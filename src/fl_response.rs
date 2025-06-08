use std::{collections::HashMap, fmt::Debug};

use hyper::{header::CONNECTION, StatusCode};
use serde::de::DeserializeOwned;
use url_utils::UrlBuilder;

use crate::{
    fl_response_as_stream::FlResponseAsStream, FlUrlError, FlUrlReadingHeaderError, ResponseBody,
};

pub struct FlUrlResponse {
    pub url: UrlBuilder,
    status_code: StatusCode,
    response: ResponseBody,
}

impl Debug for FlUrlResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlUrlResponse")
            .field("url", &self.url.to_string())
            .field("status_code", &self.status_code)
            .finish()
    }
}

impl FlUrlResponse {
    pub fn from_http1_response<
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + 'static,
    >(
        url: UrlBuilder,
        response: my_http_client::http1::MyHttpResponse<TStream>,
    ) -> Self {
        Self {
            status_code: response.status(),
            response: ResponseBody::Hyper(Some(response.into_response())),
            url,
        }
    }

    pub fn drop_connection(&self) -> bool {
        let header = self.response.get_header(CONNECTION.as_str());
        if let Ok(header) = header {
            if let Some(header) = header {
                return header.eq_ignore_ascii_case("close");
            }
        }
        false
    }

    pub fn into_hyper_response(self) -> my_http_client::HyperResponse {
        self.response.into_hyper_response()
    }

    pub fn get_header(&self, name: &str) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        self.response.get_header(name)
    }

    pub fn get_header_case_insensitive(
        &self,
        name: &str,
    ) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        self.response.get_header_case_insensitive(name)
    }

    pub fn get_headers(&self) -> HashMap<&str, Option<&str>> {
        let mut result = HashMap::new();

        self.response.copy_headers_to_hash_map(&mut result);

        result
    }

    pub fn fill_headers_to_hashmap_of_string(&self, dest: &mut HashMap<String, Option<String>>) {
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

    pub async fn get_body_as_str(&mut self) -> Result<&str, FlUrlError> {
        let bytes = self.response.convert_body_and_get_as_slice().await?;
        Ok(std::str::from_utf8(bytes)?)
    }

    pub fn get_body_as_stream(self) -> FlResponseAsStream {
        let response = match self.response {
            ResponseBody::Hyper(response) => response.unwrap(),
            ResponseBody::Body { .. } => {
                panic!("Can not get body as stream when body is materialized");
            }
        };
        FlResponseAsStream::new(self.url, response)
    }

    #[deprecated(note = "Use get_body_as_str")]
    pub async fn body_as_str(&mut self) -> Result<&str, FlUrlError> {
        let bytes = self.response.convert_body_and_get_as_slice().await?;
        Ok(std::str::from_utf8(bytes)?)
    }

    pub fn get_status_code(&self) -> u16 {
        self.status_code.as_u16()
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();

        result.push_str("StatusCode: ");
        result.push_str(self.status_code.as_u16().to_string().as_str());

        result.push_str("; ");

        result.push_str("Headers: ");

        let mut headers = HashMap::new();
        self.response.copy_headers_to_hash_map(&mut headers);

        for (key, value) in headers {
            result.push_str(key);
            result.push_str("='");
            if let Some(value) = value {
                result.push_str(value);
            }
            result.push_str("';");
        }

        result
    }
}
