use std::{collections::HashMap, str::FromStr, time::Duration};

use bytes::Bytes;
use http_body_util::Full;
use hyper::{client::conn::http1::SendRequest, Method, Request, Uri};

use rust_extensions::StrOrString;
use tokio::sync::Mutex;

use crate::{ClientCertificate, FlUrlError, FlUrlResponse, UrlBuilder};

pub struct HttpClient {
    connection: Mutex<Option<SendRequest<Full<Bytes>>>>,
    host: String,
}

impl HttpClient {
    pub async fn new(
        src: &UrlBuilder,
        client_certificate: Option<ClientCertificate>,
        request_timeout: Duration,
    ) -> Result<Self, FlUrlError> {
        let host_port = src.get_host_port();

        let domain = src.get_domain();

        let is_https = src.scheme.is_https();

        let host_port: StrOrString = if host_port.contains(":") {
            host_port.into()
        } else {
            if is_https {
                format!("{}:443", host_port).into()
            } else {
                format!("{}:80", host_port).into()
            }
        };

        let connection = if is_https {
            super::connect_to_tls_endpoint(
                host_port.as_str(),
                domain,
                request_timeout,
                client_certificate,
            )
            .await?
        } else {
            super::connect_to_http_endpoint(host_port.as_str(), request_timeout).await?
        };
        let result = Self {
            connection: Mutex::new(Some(connection)),
            host: domain.to_string(),
        };

        Ok(result)
    }

    pub async fn execute_request(
        &self,
        url_builder: &UrlBuilder,
        method: Method,
        headers: &HashMap<String, String>,
        body: Option<Vec<u8>>,
        request_timeout: Duration,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = if let Some(body) = body {
            http_body_util::Full::new(body.into())
        } else {
            http_body_util::Full::new(hyper::body::Bytes::from(vec![]))
        };

        let url_builder_owner = url_builder.into_builder_owned();

        let uri: Uri = url_builder_owner.as_str().parse().unwrap();

        let authority = uri.authority().unwrap().clone();

        println!("Authority: {}", authority.as_str());

        let mut request = Request::builder().uri(uri).method(method);

        {
            let headers_to_add = request.headers_mut().unwrap();

            headers_to_add.insert(
                hyper::http::header::HOST,
                hyper::http::HeaderValue::from_str(authority.as_str()).unwrap(),
            );

            if headers.len() > 0 {
                for (key, value) in headers {
                    let header_name = hyper::http::HeaderName::from_str(key).unwrap();
                    headers_to_add.insert(
                        header_name,
                        hyper::http::HeaderValue::from_str(value).unwrap(),
                    );
                }
            };
        }

        let request = request.body(body)?;

        let mut access = self.connection.lock().await;

        if access.is_none() {
            return Err(FlUrlError::ConnectionIsDead);
        }

        let connection = access.as_mut().unwrap();

        let request_future = connection.send_request(request);

        let result = tokio::time::timeout(request_timeout, request_future).await;

        if result.is_err() {
            *access = None;
            return Err(FlUrlError::Timeout);
        }

        let result = result.unwrap()?;

        Ok(FlUrlResponse::new(url_builder_owner.clone(), result))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use hyper::Method;
    use rust_extensions::StopWatch;

    use super::HttpClient;
    use crate::UrlBuilder;

    static REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn test_http_request() {
        let url_builder = UrlBuilder::new("http://google.com/".into());

        let fl_url_client = HttpClient::new(&url_builder, None, REQUEST_TIMEOUT)
            .await
            .unwrap();

        let mut sw: StopWatch = StopWatch::new();

        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &HashMap::new(),
                None,
                REQUEST_TIMEOUT,
            )
            .await
            .unwrap();
        println!("StatusCode: {}", response.get_status_code());
        println!("Body: {}", response.body_as_str().await.unwrap());

        sw.pause();
        println!("Duration: {:?}", sw.duration());

        let mut sw: StopWatch = StopWatch::new();
        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &HashMap::new(),
                None,
                REQUEST_TIMEOUT,
            )
            .await
            .unwrap();
        println!("StatusCode: {}", response.get_status_code());
        println!("Body: {}", response.body_as_str().await.unwrap());

        sw.pause();
        println!("Duration: {:?}", sw.duration());
    }

    #[tokio::test]
    async fn test_https_request() {
        let url_builder = UrlBuilder::new("https://trade-demo.yourfin.tech".into());

        let fl_url_client = HttpClient::new(&url_builder, None, REQUEST_TIMEOUT)
            .await
            .unwrap();

        let mut sw: StopWatch = StopWatch::new();

        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &HashMap::new(),
                None,
                REQUEST_TIMEOUT,
            )
            .await
            .unwrap();
        println!("StatusCode: {}", response.get_status_code());
        println!("Body: {}", response.body_as_str().await.unwrap());

        sw.pause();
        println!("Duration: {:?}", sw.duration());

        let mut sw: StopWatch = StopWatch::new();
        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &HashMap::new(),
                None,
                REQUEST_TIMEOUT,
            )
            .await
            .unwrap();
        println!("StatusCode: {}", response.get_status_code());
        println!("Body: {}", response.body_as_str().await.unwrap());

        sw.pause();
        println!("Duration: {:?}", sw.duration());
    }
}
