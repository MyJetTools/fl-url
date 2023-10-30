use std::time::Duration;

use rust_extensions::StrOrString;

use crate::{FlUrl, FlUrlError, FlUrlResponse};

#[async_trait::async_trait]
pub trait IntoFlUrl<'g> {
    fn create_http_request_with_timeout(self, timeout: Duration) -> FlUrl<'g>;
    fn append_path_segment(self, path_segment: impl Into<StrOrString<'static>>) -> FlUrl<'g>;
    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl<'g>;

    fn with_header<'s, 'a>(
        self,
        name: impl Into<StrOrString<'s>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl<'g>;

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl<'g>;

    async fn get(self) -> Result<FlUrlResponse, FlUrlError>;
    async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError>;
    async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError>;
    async fn post_json(
        self,
        json: impl serde::Serialize + Send + Sync + 'static,
    ) -> Result<FlUrlResponse, FlUrlError>;
    async fn delete(self) -> Result<FlUrlResponse, FlUrlError>;
    async fn head(self) -> Result<FlUrlResponse, FlUrlError>;
}

#[async_trait::async_trait]
impl<'g> IntoFlUrl<'g> for &'g str {
    fn create_http_request_with_timeout(self, timeout: Duration) -> FlUrl<'g> {
        FlUrl::new_with_timeout(self, timeout)
    }

    fn append_path_segment(self, path_segment: impl Into<StrOrString<'static>>) -> FlUrl<'g> {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'s, 'a>(
        self,
        name: impl Into<StrOrString<'s>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl<'g> {
        FlUrl::new(self).append_raw_ending_to_url(raw)
    }

    async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).get().await
    }

    async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).head().await
    }

    async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post(body).await
    }

    async fn post_json(
        self,
        json: impl serde::Serialize + Send + Sync + 'static,
    ) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post_json(json).await
    }

    async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).put(body).await
    }

    async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).delete().await
    }
}

#[async_trait::async_trait]
impl<'g> IntoFlUrl<'g> for &'g String {
    fn create_http_request_with_timeout(self, timeout: Duration) -> FlUrl<'g> {
        FlUrl::new_with_timeout(self, timeout)
    }
    fn append_path_segment(self, path_segment: impl Into<StrOrString<'static>>) -> FlUrl<'g> {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'s, 'a>(
        self,
        name: impl Into<StrOrString<'s>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl<'g> {
        FlUrl::new(self).append_raw_ending_to_url(raw)
    }

    async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).get().await
    }

    async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).head().await
    }

    async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post(body).await
    }

    async fn post_json(
        self,
        json: impl serde::Serialize + Send + Sync + 'static,
    ) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post_json(json).await
    }

    async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).put(body).await
    }

    async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).delete().await
    }
}

#[async_trait::async_trait]
impl<'g> IntoFlUrl<'g> for String {
    fn create_http_request_with_timeout(self, timeout: Duration) -> FlUrl<'g> {
        FlUrl::new_with_timeout(self, timeout)
    }

    fn append_path_segment(self, path_segment: impl Into<StrOrString<'static>>) -> FlUrl<'g> {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: impl Into<StrOrString<'v>>,
    ) -> FlUrl<'g> {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl<'g> {
        FlUrl::new(self).append_raw_ending_to_url(raw)
    }

    async fn get(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).get().await
    }

    async fn head(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).head().await
    }

    async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post(body).await
    }

    async fn post_json(
        self,
        json: impl serde::Serialize + Send + Sync + 'static,
    ) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).post_json(json).await
    }

    async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).put(body).await
    }

    async fn delete(self) -> Result<FlUrlResponse, FlUrlError> {
        FlUrl::new(self).delete().await
    }
}
