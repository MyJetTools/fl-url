use std::collections::HashMap;
use std::fmt::Debug;

use serde::de::DeserializeOwned;
use my_http_utils::UrlBuilder;

use crate::wasm::fetch::{collect_headers, read_response_body};
use crate::{FlUrlError, FlUrlReadingHeaderError};

/// wasm counterpart of the native `FlUrlResponse`.
///
/// Status and headers are read eagerly from the `fetch` `Response`; the body is
/// read (and cached) lazily on the first `get_body_*` / `get_json` /
/// `receive_body` call, matching the native lazy-load semantics.
pub struct FlUrlResponse {
    pub url: UrlBuilder,
    status_code: u16,
    headers: Vec<(String, String)>,
    // Consumed the first time the body is read; `array_buffer()` can only run once.
    response: Option<web_sys::Response>,
    // The request's AbortController, reused to bound the body read when a
    // response-body timeout is configured.
    controller: Option<web_sys::AbortController>,
    body_timeout_millis: Option<i32>,
    body: Option<Vec<u8>>,
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
    pub(crate) fn new(
        url: UrlBuilder,
        response: web_sys::Response,
        controller: Option<web_sys::AbortController>,
        body_timeout_millis: Option<i32>,
    ) -> Self {
        let status_code = response.status();
        let headers = collect_headers(&response.headers());
        Self {
            url,
            status_code,
            headers,
            response: Some(response),
            controller,
            body_timeout_millis,
            body: None,
        }
    }

    async fn load_body(&mut self) -> Result<(), FlUrlError> {
        if self.body.is_some() {
            return Ok(());
        }
        let response = self.response.take().ok_or_else(|| {
            FlUrlError::FetchError("response body has already been consumed".to_string())
        })?;
        let bytes =
            read_response_body(&response, self.controller.as_ref(), self.body_timeout_millis)
                .await?;
        self.body = Some(bytes);
        Ok(())
    }

    pub fn get_status_code(&self) -> u16 {
        self.status_code
    }

    pub fn get_header(&self, name: &str) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        // The browser lower-cases response header names, and the native backend's
        // `get_header` is case-insensitive too (hyper normalizes header names), so
        // we match case-insensitively to keep the same call portable across
        // targets.
        self.get_header_case_insensitive(name)
    }

    pub fn get_header_case_insensitive(
        &self,
        name: &str,
    ) -> Result<Option<&str>, FlUrlReadingHeaderError> {
        Ok(self
            .headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str()))
    }

    pub fn get_headers(&self) -> HashMap<&str, Option<&str>> {
        self.headers
            .iter()
            .map(|(name, value)| (name.as_str(), Some(value.as_str())))
            .collect()
    }

    pub fn fill_headers_to_hashmap_of_string(&self, dest: &mut HashMap<String, Option<String>>) {
        for (name, value) in &self.headers {
            dest.insert(name.clone(), Some(value.clone()));
        }
    }

    pub async fn get_body_as_slice(&mut self) -> Result<&[u8], FlUrlError> {
        self.load_body().await?;
        Ok(self.body.as_ref().unwrap().as_slice())
    }

    pub async fn get_json<TResponse: DeserializeOwned>(&mut self) -> Result<TResponse, FlUrlError> {
        self.load_body().await?;
        let body = self.body.as_ref().unwrap().as_slice();
        let result = serde_json::from_slice(body)?;
        Ok(result)
    }

    pub async fn receive_body(mut self) -> Result<Vec<u8>, FlUrlError> {
        self.load_body().await?;
        Ok(self.body.take().unwrap())
    }

    pub async fn get_body_as_str(&mut self) -> Result<&str, FlUrlError> {
        self.load_body().await?;
        let bytes = self.body.as_ref().unwrap().as_slice();
        Ok(std::str::from_utf8(bytes)?)
    }

    #[deprecated(note = "Use get_body_as_str")]
    pub async fn body_as_str(&mut self) -> Result<&str, FlUrlError> {
        self.get_body_as_str().await
    }

    /// Always `false` under wasm — kept for API parity. The browser owns the
    /// connection, so FlUrl never decides to drop it.
    pub fn drop_connection(&self) -> bool {
        false
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();

        result.push_str("StatusCode: ");
        result.push_str(self.status_code.to_string().as_str());
        result.push_str("; ");
        result.push_str("Headers: ");

        for (name, value) in &self.headers {
            result.push_str(name);
            result.push_str("='");
            result.push_str(value);
            result.push_str("';");
        }

        result
    }
}
