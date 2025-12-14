use std::sync::Arc;

use my_http_client::{MyHttpClientConnector, MyHttpClientError};
use my_tls::{
    tokio_rustls::{client::TlsStream, TlsConnector},
    ClientCertificate,
};
use rust_extensions::remote_endpoint::{RemoteEndpoint, RemoteEndpointOwned};
use tokio::net::TcpStream;

pub struct HttpsConnector {
    pub remote_host: RemoteEndpointOwned,
    pub server_name: String,
    pub client_certificate: Option<ClientCertificate>,
    h2: bool,
}

impl HttpsConnector {
    pub fn new(
        remote_host: RemoteEndpointOwned,
        server_name: String,
        client_certificate: Option<ClientCertificate>,
        h2: bool,
    ) -> Self {
        Self {
            remote_host,
            server_name,
            client_certificate,
            h2,
        }
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<TlsStream<TcpStream>> for HttpsConnector {
    async fn connect(&self) -> Result<TlsStream<TcpStream>, MyHttpClientError> {
        let host_port = self.remote_host.get_host_port();
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

        let mut client_config = client_config.unwrap();

        if self.h2 {
            client_config.alpn_protocols = vec![b"h2".to_vec()];
        }

        let connector = TlsConnector::from(Arc::new(client_config));

        let server_name = my_tls::tokio_rustls::rustls::pki_types::ServerName::try_from(
            self.server_name.to_string(),
        )
        .unwrap();

        match connector.connect(server_name, tcp_stream).await {
            Ok(tls_stream) => Ok(tls_stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    host_port, err
                )),
            ),
        }
    }

    fn get_remote_endpoint<'s>(&'s self) -> RemoteEndpoint<'s> {
        self.remote_host.to_ref()
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
