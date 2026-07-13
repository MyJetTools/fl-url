use std::{collections::HashMap, fmt::Debug, time::Duration};

use hyper::{header::CONNECTION, StatusCode};
use serde::de::DeserializeOwned;
use url_utils::UrlBuilder;

use crate::{
    fl_response_as_stream::FlResponseAsStream, ConnectionReturner, FlUrlError,
    FlUrlReadingHeaderError, ResponseBody,
};

pub struct FlUrlResponse {
    pub url: UrlBuilder,
    status_code: StatusCode,
    response: ResponseBody,
    body_read_timeout: Option<Duration>,
    decompress_gzip: bool,
    // Owns the checked-out connection until the body is fully consumed. Dropped
    // without returning (dispose) on error, `Connection: close`, or when the
    // response is discarded with the body unread.
    connection_returner: Option<Box<dyn ConnectionReturner>>,
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
            body_read_timeout: None,
            decompress_gzip: false,
            connection_returner: None,
        }
    }

    pub(crate) fn set_body_read_timeout(&mut self, timeout: Option<Duration>) {
        self.body_read_timeout = timeout;
    }

    pub(crate) fn set_decompress_gzip(&mut self, decompress_gzip: bool) {
        self.decompress_gzip = decompress_gzip;
    }

    pub(crate) fn set_connection_returner(&mut self, returner: Box<dyn ConnectionReturner>) {
        self.connection_returner = Some(returner);
    }

    /// Loads the body into memory and settles the checked-out connection: a
    /// healthy connection goes back to the pool, a broken one (read error,
    /// `Connection: close`, drop-worthy status) gets disposed.
    async fn load_body(&mut self) -> Result<(), FlUrlError> {
        let load_result = self
            .response
            .convert_to_slice_if_needed(self.body_read_timeout)
            .await;

        match load_result {
            Ok(()) => {
                // Ok(()) alone does not prove the body was read off the wire:
                // after a CANCELLED earlier read the enum is already the
                // materialized variant with body: None, and re-entry returns Ok
                // without touching the socket. Only a body that actually loaded
                // makes the connection safe to reuse.
                if self.response.has_loaded_body() {
                    self.release_connection().await;
                } else {
                    self.connection_returner.take();
                }
                // Gzip decoding is a body-level transform run AFTER the
                // connection is settled: a decode failure is a data error, not a
                // connection problem — the socket was already fully drained.
                if self.decompress_gzip {
                    self.response.decode_gzip_if_needed()?;
                }
                Ok(())
            }
            Err(err) => {
                // Dropping the returner disposes the connection: its socket
                // still carries the unread remainder of this body.
                self.connection_returner.take();
                Err(err)
            }
        }
    }

    async fn release_connection(&mut self) {
        let Some(returner) = self.connection_returner.take() else {
            return;
        };

        let close_requested = self.drop_connection();
        let drop_by_status =
            crate::fl_drop_connection_scenario::should_drop_connection_by_status(
                self.get_status_code(),
            );

        if !close_requested && !drop_by_status {
            returner.return_connection().await;
        }
        // else: dropping the returner disposes the connection
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

    pub fn into_hyper_response(mut self) -> my_http_client::HyperResponse {
        use http_body_util::BodyExt;

        let returner = self.connection_returner.take();
        let close_requested = self.drop_connection();
        let drop_by_status = crate::fl_drop_connection_scenario::should_drop_connection_by_status(
            self.get_status_code(),
        );

        let response = self.response.into_hyper_response();

        // The body escapes fl-url's control while still streaming from the
        // checked-out connection: keep the connection alive via a body guard
        // that settles it on end-of-stream instead of disposing it mid-body.
        match returner {
            None => response,
            Some(returner) => response.map(|body| {
                crate::escaped_body_guard::EscapedBodyGuard::new(
                    body,
                    returner,
                    !close_requested && !drop_by_status,
                )
                .boxed()
            }),
        }
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
        self.load_body().await?;
        self.response.get_loaded_body_as_slice()
    }

    pub async fn get_json<TResponse: DeserializeOwned>(&mut self) -> Result<TResponse, FlUrlError> {
        self.load_body().await?;
        let body = self.response.get_loaded_body_as_slice()?;
        let result = serde_json::from_slice(body)?;
        Ok(result)
    }

    pub async fn receive_body(mut self) -> Result<Vec<u8>, FlUrlError> {
        self.load_body().await?;
        self.response.take_loaded_body()
    }

    pub async fn get_body_as_str(&mut self) -> Result<&str, FlUrlError> {
        self.load_body().await?;
        let bytes = self.response.get_loaded_body_as_slice()?;
        Ok(std::str::from_utf8(bytes)?)
    }

    pub fn get_body_as_stream(self) -> FlResponseAsStream {
        let response = match self.response {
            ResponseBody::Hyper(response) => response.unwrap(),
            ResponseBody::Body { .. } => {
                panic!("Can not get body as stream when body is materialized");
            }
        };
        FlResponseAsStream::create(
            self.url,
            response,
            self.body_read_timeout,
            self.connection_returner,
        )
    }

    #[deprecated(note = "Use get_body_as_str")]
    pub async fn body_as_str(&mut self) -> Result<&str, FlUrlError> {
        self.load_body().await?;
        let bytes = self.response.get_loaded_body_as_slice()?;
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
