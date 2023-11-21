use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use tokio_rustls::{rustls, TlsConnector};

use crate::FlUrlError;

use super::cert_content::ROOT_CERT_STORE;

pub async fn connect_to_tls_endpoint(
    host_port: &str,
    domain: &str,
    request_timeout: Duration,
) -> Result<SendRequest<Full<Bytes>>, FlUrlError> {
    loop {
        let connect = TcpStream::connect(host_port);

        let connect_result = tokio::time::timeout(request_timeout, connect).await;

        if connect_result.is_err() {
            println!("Timeout while connecting to http://{}", host_port);
            return Err(FlUrlError::Timeout);
        }
        let connect_result = connect_result.unwrap();

        match connect_result {
            Ok(tcp_stream) => {
                let config = rustls::ClientConfig::builder()
                    .with_safe_defaults()
                    .with_root_certificates(ROOT_CERT_STORE.clone())
                    .with_no_client_auth();
                let connector = TlsConnector::from(Arc::new(config));

                let domain = rustls::ServerName::try_from(domain).unwrap();

                let tls_stream = connector.connect(domain, tcp_stream).await?;

                let io = TokioIo::new(tls_stream);

                let handshake_result = hyper::client::conn::http1::handshake(io).await;
                match handshake_result {
                    Ok((sender, conn)) => {
                        let host_port = host_port.to_owned();
                        tokio::task::spawn(async move {
                            if let Err(err) = conn.await {
                                println!(
                                    "Http Connection to http://{} is failed: {:?}",
                                    host_port, err
                                );
                            }
                        });

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
}
