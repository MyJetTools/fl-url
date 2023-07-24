use hyper::{client::HttpConnector, Body, Client, Request};
use hyper_rustls::HttpsConnector;

use crate::{ClientCertificate, FlUrlError, FlUrlResponse, UrlBuilder};

pub enum FlUrlClient {
    Http(Client<HttpConnector>),
    Https(Client<HttpsConnector<HttpConnector>>),
}

impl FlUrlClient {
    pub fn new(is_https: bool, cert: Option<ClientCertificate>) -> Self {
        if is_https {
            if let Some(client_cert) = cert {
                Self::new_https_with_client_cert(client_cert)
            } else {
                Self::new_https()
            }
        } else {
            Self::new_http()
        }
    }
    pub fn new_http() -> Self {
        Self::Http(Client::builder().build_http())
    }

    pub fn new_https() -> Self {
        use hyper_rustls::ConfigBuilderExt;

        let tls = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_native_roots()
            .with_no_client_auth();

        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();

        let client = hyper::client::Client::builder().build(https);

        Self::Https(client)
    }

    pub fn new_https_with_client_cert(client_cert: ClientCertificate) -> Self {
        use hyper_rustls::ConfigBuilderExt;
        let tls = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_native_roots()
            .with_client_auth_cert(vec![client_cert.cert], client_cert.pkey)
            .unwrap();

        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();

        let client = hyper::client::Client::builder().build(https);
        Self::Https(client)
    }

    pub async fn execute(
        &self,
        url: UrlBuilder,
        request: Request<Body>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        match self {
            FlUrlClient::Http(client) => {
                let result = match client.request(request).await {
                    Ok(response) => Ok(FlUrlResponse::new(url, response)),
                    Err(err) => Err(err),
                };

                return Ok(result?);
            }
            FlUrlClient::Https(client) => {
                let result = match client.request(request).await {
                    Ok(response) => Ok(FlUrlResponse::new(url, response)),
                    Err(err) => Err(err),
                };

                return Ok(result?);
            }
        }
    }
}
