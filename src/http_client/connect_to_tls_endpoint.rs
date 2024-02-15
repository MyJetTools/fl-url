use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use tokio_rustls::rustls::pki_types;
use tokio_rustls::{rustls, TlsConnector};

use crate::FlUrlError;

use my_tls::ClientCertificate;

use my_tls::ROOT_CERT_STORE;

pub async fn connect_to_tls_endpoint(
    host_port: &str,
    domain: &str,
    client_certificate: Option<ClientCertificate>,
) -> Result<SendRequest<Full<Bytes>>, FlUrlError> {
    let connect_result = TcpStream::connect(host_port).await;

    match connect_result {
        Ok(tcp_stream) => {
            let config_builder =
                rustls::ClientConfig::builder().with_root_certificates(ROOT_CERT_STORE.clone());

            let client_config = if let Some(client_cert) = client_certificate {
                let result = config_builder
                    .with_client_auth_cert(client_cert.cert_chain, client_cert.private_key);

                match result {
                    Ok(config) => config,
                    Err(err) => return Err(FlUrlError::ClientCertificateError(err)),
                }
            } else {
                config_builder.with_no_client_auth()
            };

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
