use std::sync::Arc;

use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use my_tls::{
    tokio_rustls::{client::TlsStream, TlsConnector},
    ClientCertificate,
};
use rust_extensions::{remote_endpoint::RemoteEndpointOwned, StrOrString};
use tokio::net::TcpStream;

pub struct HttpsConnector {
    pub remote_host: RemoteEndpointOwned,
    pub domain: Option<String>,
    pub client_certificate: Option<ClientCertificate>,
}

impl HttpsConnector {
    pub fn new(
        remote_host: RemoteEndpointOwned,
        domain: Option<String>,
        client_certificate: Option<ClientCertificate>,
    ) -> Self {
        Self {
            remote_host,
            domain,
            client_certificate,
        }
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<TlsStream<TcpStream>> for HttpsConnector {
    async fn connect(&self) -> Result<TlsStream<TcpStream>, MyHttpClientError> {
        let host_port = self.remote_host.get_host_port(Some(443));
        let connect_result = TcpStream::connect(host_port.as_str()).await;

        if let Err(err) = &connect_result {
            return Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    host_port, err
                )),
            );
        }

        let tcp_stream = connect_result.unwrap();

        let client_config = my_tls::create_tls_client_config(&self.client_certificate);

        if let Err(err) = client_config {
            return Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    host_port, err
                )),
            );
        }

        let client_config = client_config.unwrap();

        let connector = TlsConnector::from(Arc::new(client_config));

        let domain = if let Some(domain) = self.domain.as_ref() {
            my_tls::tokio_rustls::rustls::pki_types::ServerName::try_from(domain.to_string())
                .unwrap()
        } else {
            my_tls::tokio_rustls::rustls::pki_types::ServerName::try_from(
                self.remote_host.get_host().to_string(),
            )
            .unwrap()
        };

        match connector.connect(domain, tcp_stream).await {
            Ok(tls_stream) => Ok(tls_stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    host_port, err
                )),
            ),
        }
    }

    fn get_remote_host(&self) -> StrOrString {
        self.remote_host.as_str().into()
    }
    fn is_debug(&self) -> bool {
        false
    }

    fn reunite(
        _read: tokio::io::ReadHalf<TlsStream<TcpStream>>,
        _write: tokio::io::WriteHalf<TlsStream<TcpStream>>,
    ) -> TlsStream<TcpStream> {
        panic!("Would implement this if upgrade fl-url to support WebSockets")
    }
}