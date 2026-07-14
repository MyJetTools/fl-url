//! A no-op request model used with `FlUrl::execute_request` when a request
//! carries no input model.

use my_http_utils::body::HttpRequestBody;
use my_http_utils::schema::client::{
    HeaderBuilder, HttpRequestBuildError, RandomStringGenerator, THttpRequestBuilder,
};
use my_http_utils::UrlBuilder;

/// A stub [`THttpRequestBuilder`] that describes an empty request: it appends
/// nothing to the URL, adds no headers and produces an empty body.
///
/// Pass it to `FlUrl::execute_request` for a parameter-less request so you don't
/// have to derive a dedicated `#[derive(MyHttpInput)]` model just to satisfy the
/// signature. The URL and headers already set on the `FlUrl` (via
/// `append_path_segment`, `with_header`, …) are used as-is; body-carrying verbs
/// send an empty body.
///
/// ```ignore
/// FlUrl::new("https://api.example.com")
///     .append_path_segment("health")
///     .execute_request(HttpVerb::Get, EmptyRequestModel)
///     .await?;
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyRequestModel;

impl THttpRequestBuilder for EmptyRequestModel {
    fn fill_url(&self, _url_builder: &mut UrlBuilder) -> Result<(), HttpRequestBuildError> {
        Ok(())
    }

    fn fill_headers(&self, _headers: &mut impl HeaderBuilder) -> Result<(), HttpRequestBuildError> {
        Ok(())
    }

    fn get_body<TRnd: RandomStringGenerator>(
        self,
    ) -> Result<HttpRequestBody, HttpRequestBuildError> {
        Ok(HttpRequestBody::Empty)
    }
}
