use rust_extensions::StrOrString;

use crate::{FlUrl, FlUrlError, FlUrlResponse};

#[async_trait::async_trait]
pub trait IntoFlUrl {
    fn append_path_segment<'s>(self, path_segment: impl Into<StrOrString<'s>>) -> FlUrl;
    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl;

    fn with_header<'a>(
        self,
        name: impl Into<StrOrString<'static>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl;

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl;

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
impl<'g> IntoFlUrl for &'g str {
    fn append_path_segment<'s>(self, path_segment: impl Into<StrOrString<'s>>) -> FlUrl {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'a>(
        self,
        name: impl Into<StrOrString<'static>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl {
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
impl<'g> IntoFlUrl for &'g String {
    fn append_path_segment<'s>(self, path_segment: impl Into<StrOrString<'s>>) -> FlUrl {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'a>(
        self,
        name: impl Into<StrOrString<'static>>,
        value: impl Into<StrOrString<'a>>,
    ) -> FlUrl {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl {
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
impl IntoFlUrl for String {
    fn append_path_segment<'s>(self, path_segment: impl Into<StrOrString<'s>>) -> FlUrl {
        FlUrl::new(self).append_path_segment(path_segment)
    }

    fn append_query_param<'n, 'v>(
        self,
        name: impl Into<StrOrString<'n>>,
        value: Option<impl Into<StrOrString<'v>>>,
    ) -> FlUrl {
        FlUrl::new(self).append_query_param(name, value)
    }

    fn with_header<'v>(
        self,
        name: impl Into<StrOrString<'static>>,
        value: impl Into<StrOrString<'v>>,
    ) -> FlUrl {
        FlUrl::new(self).with_header(name, value)
    }

    fn append_raw_ending_to_url<'s>(self, raw: impl Into<StrOrString<'s>>) -> FlUrl {
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
