use hyper::{client::HttpConnector, Body, Client, Request};
use hyper_rustls::HttpsConnector;

use crate::{ClientCertificate, FlUrlError, FlUrlResponse, UrlBuilder};

pub enum FlUrlClient {
    Http(Client<HttpConnector>),
    Https(Client<HttpsConnector<HttpConnector>>),
    #[cfg(feature = "support-unix-socket")]
    UnixSocket(Client<hyper_unix_connector::UnixClient>),
}

impl FlUrlClient {
    #[cfg(feature = "support-unix-socket")]
    pub fn new_unix_socket() -> Self {
        let client: Client<hyper_unix_connector::UnixClient, Body> =
            Client::builder().build(hyper_unix_connector::UnixClient);
        Self::UnixSocket(client)
    }

    pub fn new_http() -> Self {
        Self::Http(Client::builder().build_http())
    }

    pub fn new_https(client_certificate: Option<ClientCertificate>) -> Self {
        use hyper_rustls::ConfigBuilderExt;

        let client_connector = if let Some(client_cert) = client_certificate {
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_native_roots()
                .with_client_auth_cert(vec![client_cert.cert], client_cert.pkey)
                .unwrap()
        } else {
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_native_roots()
                .with_no_client_auth()
        };

        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(client_connector)
            .https_or_http()
            .enable_http1()
            .build();

        let client = hyper::client::Client::builder();

        Self::Https(client.build(https_connector))
    }

    pub async fn execute(
        &self,
        url: UrlBuilder,
        request: Request<Body>,
    ) -> Result<FlUrlResponse, FlUrlError> {
        match self {
            FlUrlClient::Http(client) => {
                let response = client.request(request).await?;
                return Ok(FlUrlResponse::new(url, response));
            }
            FlUrlClient::Https(client) => {
                let response = client.request(request).await?;
                return Ok(FlUrlResponse::new(url, response));
            }
            #[cfg(feature = "support-unix-socket")]
            FlUrlClient::UnixSocket(client) => {
                let response = client.request(request).await?;

                return Ok(FlUrlResponse::new(url, response));
            }
        }
    }
}
