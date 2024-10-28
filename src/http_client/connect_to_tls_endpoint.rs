use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use my_tls::tokio_rustls::rustls::client::ResolvesClientCert;
use my_tls::tokio_rustls::rustls::pki_types;
use my_tls::tokio_rustls::{rustls, TlsConnector};

use crate::FlUrlError;

use my_tls::ClientCertificate;

pub async fn connect_to_tls_endpoint(
    host_port: &str,
    domain: &str,
    client_certificate: &Option<ClientCertificate>,
) -> Result<SendRequest<Full<Bytes>>, FlUrlError> {
    let connect_result = TcpStream::connect(host_port).await;

    match connect_result {
        Ok(tcp_stream) => {
            let client_config = my_tls::create_tls_client_config(client_certificate)?;
            let connector = TlsConnector::from(Arc::new(client_config));

            let domain = pki_types::ServerName::try_from(domain.to_string()).unwrap();

            let tls_stream = connector.connect(domain, tcp_stream).await?;

            let io = TokioIo::new(tls_stream);

            let handshake_result = hyper::client::conn::http1::handshake(io).await;

            match handshake_result {
                Ok((mut sender, conn)) => {
                    let host_port = host_port.to_owned();
                    tokio::task::spawn(async move {
                        if let Err(err) = conn.await {
                            println!(
                                "Https Connection to https://{} is failed: {:?}",
                                host_port, err
                            );
                        }
                    });

                    sender.ready().await?;

                    return Ok(sender);
                }
                Err(err) => {
                    return Err(FlUrlError::InvalidHttp1HandShake(format!("{}", err)));
                }
            }
        }
        Err(err) => {
            return Err(FlUrlError::CanNotEstablishConnection(format!("{}", err)));
        }
    }
}

#[derive(Debug)]
pub struct MyClientCertResolver(Arc<my_tls::tokio_rustls::rustls::sign::CertifiedKey>);

impl ResolvesClientCert for MyClientCertResolver {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[rustls::SignatureScheme],
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(self.0.clone())
    }

    fn has_certs(&self) -> bool {
        true
    }
}
