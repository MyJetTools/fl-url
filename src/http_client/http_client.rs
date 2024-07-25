use std::{sync::atomic::AtomicBool, time::Duration};

use bytes::Bytes;
use http_body_util::Full;
use hyper::{client::conn::http1::SendRequest, Method, Request, Uri};

use rust_extensions::{date_time::DateTimeAsMicroseconds, MaybeShortString, StrOrString};
use tokio::sync::Mutex;

use crate::{FlUrlError, FlUrlHeaders, FlUrlResponse, UrlBuilder, UrlBuilderOwned};

use my_tls::ClientCertificate;

const DEAD_CONNECTION_DURATION: Duration = Duration::from_secs(20);

pub struct HttpClient {
    connection: Mutex<Option<SendRequest<Full<Bytes>>>>,
    pub created: DateTimeAsMicroseconds,
    disconnected: AtomicBool,

    #[cfg(feature = "with-ssh")]
    _ssh_session: Option<std::sync::Arc<my_ssh::SshSession>>,
}

impl HttpClient {
    pub fn connection_can_be_disposed(&self) -> bool {
        let now = DateTimeAsMicroseconds::now();
        now.duration_since(self.created).as_positive_or_zero() > DEAD_CONNECTION_DURATION
    }

    #[cfg(feature = "with-ssh")]
    pub fn from_ssh_session(
        connection: SendRequest<Full<Bytes>>,
        ssh_session: std::sync::Arc<my_ssh::SshSession>,
    ) -> Self {
        Self {
            connection: Mutex::new(Some(connection)),
            created: DateTimeAsMicroseconds::now(),
            disconnected: AtomicBool::new(false),
            _ssh_session: Some(ssh_session),
        }
    }

    pub async fn new(
        src: &UrlBuilder,
        client_certificate: Option<ClientCertificate>,
        request_timeout: Duration,
        #[cfg(feature = "with-ssh")] ssh_target: Option<&crate::ssh::SshTarget>,
    ) -> Result<Self, FlUrlError> {
        #[cfg(feature = "with-ssh")]
        if let Some(ssh_target) = ssh_target {
            let host_port = src.get_host_port();

            let (host, port) = match host_port.find(':') {
                Some(index) => {
                    let host = &host_port[0..index];
                    let port = &host_port[index + 1..];

                    (host, port.parse::<u16>().unwrap())
                }
                None => (host_port, 80),
            };

            let (ssh_session, connection) =
                super::connect_to_http_over_ssh::connect_to_http_over_ssh(
                    ssh_target,
                    host,
                    port,
                    request_timeout,
                )
                .await?;

            let result = Self {
                connection: Mutex::new(Some(connection)),
                created: DateTimeAsMicroseconds::now(),
                disconnected: AtomicBool::new(false),
                #[cfg(feature = "with-ssh")]
                _ssh_session: Some(ssh_session),
            };

            return Ok(result);
        }

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
            let connection_future =
                super::connect_to_tls_endpoint(host_port.as_str(), domain, client_certificate);
            let result = tokio::time::timeout(request_timeout, connection_future).await;

            if result.is_err() {
                return Err(FlUrlError::Timeout);
            }

            result.unwrap()?
        } else {
            let connection_future = super::connect_to_http_endpoint(host_port.as_str());
            let result = tokio::time::timeout(request_timeout, connection_future).await;

            if result.is_err() {
                return Err(FlUrlError::Timeout);
            }

            result.unwrap()?
        };
        let result = Self {
            connection: Mutex::new(Some(connection)),
            created: DateTimeAsMicroseconds::now(),
            disconnected: AtomicBool::new(false),
            #[cfg(feature = "with-ssh")]
            _ssh_session: None,
        };

        Ok(result)
    }

    pub async fn execute_request(
        &self,
        url_builder: &UrlBuilder,
        method: Method,
        headers: &FlUrlHeaders,
        body: Option<Vec<u8>>,
        request_timeout: Duration,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let mut attempt_no = 0;
        let url_builder_owned = url_builder.into_builder_owned();
        loop {
            let result = self
                .execute_int(
                    &url_builder_owned,
                    &method,
                    &headers,
                    body.clone(),
                    request_timeout,
                )
                .await;

            if result.is_ok() {
                return result;
            }

            if let Err(FlUrlError::HyperError(err)) = &result {
                // This error we get if TLS Handshake is not finished yet. We are retrying after 50ms 100 times which is 5 seconds.
                // Sometime this error appears when we have this connection for the long time. I assume - this is because connection is already dead.
                if err.is_canceled() {
                    if self.connection_can_be_disposed() {
                        self.disconnect().await;
                        return result;
                    }

                    tokio::time::sleep(Duration::from_millis(50)).await;
                    attempt_no += 1;

                    if attempt_no > 100 {
                        return result;
                    }

                    continue;
                }
            }

            return result;
        }
    }

    async fn execute_int(
        &self,
        url_builder: &UrlBuilderOwned,
        method: &Method,
        headers: &FlUrlHeaders,
        body: Option<Vec<u8>>,
        request_timeout: Duration,
    ) -> Result<FlUrlResponse, FlUrlError> {
        let body = if let Some(body) = body {
            http_body_util::Full::new(body.into())
        } else {
            http_body_util::Full::new(hyper::body::Bytes::from(vec![]))
        };

        let uri: Uri = url_builder.as_str().parse().unwrap();

        let authority = MaybeShortString::from_str(uri.authority().unwrap().as_str());

        let mut request = Request::builder().uri(uri).method(method);

        {
            if !headers.has_host_header {
                request.headers_mut().unwrap().insert(
                    hyper::http::header::HOST,
                    hyper::http::HeaderValue::from_str(authority.as_str()).unwrap(),
                );
            }

            if headers.len() > 0 {
                for header in headers.iter() {
                    request = request.header(header.name.as_str(), header.value.to_string());
                }
            };
        }

        #[cfg(feature = "debug-request")]
        {
            println!("Request: {:?}", request);
        }

        let request = request.body(body)?;

        let request_future = {
            let mut access = self.connection.lock().await;

            if access.is_none() {
                return Err(FlUrlError::ConnectionIsDead);
            }

            let connection = access.as_mut().unwrap();

            connection.send_request(request)
        };

        let result = tokio::time::timeout(request_timeout, request_future).await;

        if result.is_err() {
            self.disconnect().await;
            return Err(FlUrlError::Timeout);
        }

        let result = result.unwrap()?;

        Ok(FlUrlResponse::new(url_builder.clone(), result))
    }

    async fn disconnect(&self) {
        self.disconnected
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut access = self.connection.lock().await;
        *access = None;
    }

    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use hyper::Method;
    use rust_extensions::StopWatch;

    use super::HttpClient;
    use crate::{FlUrlHeaders, UrlBuilder};

    static REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn test_http_request() {
        let url_builder = UrlBuilder::new("http://google.com/".into());

        let fl_url_client = HttpClient::new(
            &url_builder,
            None,
            REQUEST_TIMEOUT,
            #[cfg(feature = "with-ssh")]
            None,
        )
        .await
        .unwrap();

        let mut sw: StopWatch = StopWatch::new();

        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &FlUrlHeaders::new(),
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
                &FlUrlHeaders::new(),
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

        let fl_url_client = HttpClient::new(
            &url_builder,
            None,
            REQUEST_TIMEOUT,
            #[cfg(feature = "with-ssh")]
            None,
        )
        .await
        .unwrap();

        let mut sw: StopWatch = StopWatch::new();

        sw.start();

        let mut response = fl_url_client
            .execute_request(
                &url_builder,
                Method::GET,
                &FlUrlHeaders::new(),
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
                &FlUrlHeaders::new(),
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
