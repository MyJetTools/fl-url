use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::FlUrlError;

pub async fn connect_to_http_endpoint(
    host_port: &str,
) -> Result<SendRequest<Full<Bytes>>, FlUrlError> {
    let connect_result = TcpStream::connect(host_port).await;

    match connect_result {
        Ok(tcp_stream) => {
            let io = TokioIo::new(tcp_stream);
            let handshake_result = hyper::client::conn::http1::handshake(io).await;
            match handshake_result {
                Ok((mut sender, conn)) => {
                    // sender.ready().await?;
                    let host_port = host_port.to_owned();
                    tokio::task::spawn(async move {
                        if let Err(err) = conn.await {
                            println!(
                                "Http Connection to http://{} is failed: {:?}",
                                host_port, err
                            );
                        }

                        //Here
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
