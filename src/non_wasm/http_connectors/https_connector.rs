use std::net::IpAddr;
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
    pub resolved_ip: Option<IpAddr>,
    pub server_name: String,
    pub client_certificate: Option<ClientCertificate>,
    pub accept_invalid_certificate: bool,
    h2: bool,
}

impl HttpsConnector {
    pub fn new(
        remote_host: RemoteEndpointOwned,
        resolved_ip: Option<IpAddr>,
        server_name: String,
        client_certificate: Option<ClientCertificate>,
        accept_invalid_certificate: bool,
        h2: bool,
    ) -> Self {
        Self {
            remote_host,
            resolved_ip,
            server_name,
            client_certificate,
            accept_invalid_certificate,
            h2,
        }
    }
}

#[async_trait::async_trait]
impl MyHttpClientConnector<TlsStream<TcpStream>> for HttpsConnector {
    async fn connect(&self) -> Result<TlsStream<TcpStream>, MyHttpClientError> {
        let tcp_stream = super::connect_tcp(&self.remote_host, self.resolved_ip).await?;
        let target = super::describe_target(&self.remote_host, self.resolved_ip);

        let client_config = my_tls::create_tls_client_config_ex(
            &self.client_certificate,
            self.accept_invalid_certificate,
        );

        if let Err(err) = client_config {
            return Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. Err:{}",
                    target, err
                )),
            );
        }

        let mut client_config = client_config.unwrap();

        if self.h2 {
            client_config.alpn_protocols = vec![b"h2".to_vec()];
        }

        let connector = TlsConnector::from(Arc::new(client_config));

        let server_name = match my_tls::tokio_rustls::rustls::pki_types::ServerName::try_from(
            self.server_name.to_string(),
        ) {
            Ok(server_name) => server_name,
            Err(err) => {
                return Err(
                    my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                        "Invalid TLS server name '{}'. Err:{}",
                        self.server_name, err
                    )),
                );
            }
        };

        // The TCP connection is already up at this point, so a failure here is the
        // peer rejecting the handshake — said explicitly, since "connection reset"
        // alone reads like the address could not be reached at all.
        match connector.connect(server_name, tcp_stream).await {
            Ok(tls_stream) => Ok(tls_stream),
            Err(err) => Err(
                my_http_client::MyHttpClientError::CanNotConnectToRemoteHost(format!(
                    "{}. TLS handshake failed. Err:{}",
                    target, err
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
        read: tokio::io::ReadHalf<TlsStream<TcpStream>>,
        write: tokio::io::WriteHalf<TlsStream<TcpStream>>,
    ) -> TlsStream<TcpStream> {
        read.unsplit(write)
    }
}
