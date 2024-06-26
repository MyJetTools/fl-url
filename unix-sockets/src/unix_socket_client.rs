use hyper::{Method, Request};
use rust_extensions::StrOrString;

use crate::{FlUrlUnixResponse, FlUrlUnixSocketError};

pub async fn execute_get_request(
    scheme_and_host: &str,
    path_and_query: &str,
) -> Result<(FlUrlUnixResponse, String), FlUrlUnixSocketError> {
    //  let url_builder_owned = url_builder_owned::UrlBuilderOwnedLegacy::new(url.into());
    use hyper_unix_connector::UnixClient;
    let client: hyper::Client<UnixClient, hyper::Body> = hyper::Client::builder().build(UnixClient);

    let addr: hyper::Uri = hyper_unix_connector::Uri::new(scheme_and_host, path_and_query).into();

    let result = client.get(addr).await;

    match result {
        Ok(response) => {
            let status_code: u16 = response.status().into();
            let (parts, body) = response.into_parts();

            let full_body = hyper::body::to_bytes(body).await;

            if let Err(err) = &full_body {
                return Err(FlUrlUnixSocketError::HyperError(format!("{}", err)));
            }

            let result: Vec<u8> = full_body.unwrap().into_iter().collect();

            let headers = parts.headers;

            return Ok((
                FlUrlUnixResponse::new(status_code, headers, result),
                format!("{}{}", scheme_and_host, path_and_query),
            ));
        }
        Err(err) => {
            return Err(FlUrlUnixSocketError::HyperError(format!("{}", err)));
        }
    }
}

pub async fn execute_request(
    scheme_and_host: &str,
    path_and_query: &str,
    method: &str,
    headers: impl Iterator<Item = (&StrOrString<'static>, &String)>,
    body: Option<Vec<u8>>,
) -> Result<(FlUrlUnixResponse, String), FlUrlUnixSocketError> {
    use hyper_unix_connector::UnixClient;
    let client: hyper::Client<UnixClient, hyper::Body> = hyper::Client::builder().build(UnixClient);

    let addr: hyper::Uri = hyper_unix_connector::Uri::new(scheme_and_host, path_and_query).into();

    let body = if let Some(body) = body {
        hyper::Body::from(body)
    } else {
        hyper::Body::empty()
    };

    let method = Method::from_bytes(method.as_bytes()).unwrap();
    let mut request = Request::builder().uri(addr).method(method);

    for (key, value) in headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let request = request.body(body).unwrap();

    let result = client.request(request).await;

    match result {
        Ok(response) => {
            let status_code: u16 = response.status().into();
            let (parts, body) = response.into_parts();

            let full_body = hyper::body::to_bytes(body).await;

            if let Err(err) = &full_body {
                return Err(FlUrlUnixSocketError::HyperError(format!("{}", err)));
            }

            let result: Vec<u8> = full_body.unwrap().into_iter().collect();

            let headers = parts.headers;

            return Ok((
                FlUrlUnixResponse::new(status_code, headers, result),
                format!("{}{}", scheme_and_host, path_and_query),
            ));
        }
        Err(err) => {
            return Err(FlUrlUnixSocketError::HyperError(format!("{}", err)));
        }
    }
}
