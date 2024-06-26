use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use my_ssh::SshSession;

use crate::{ssh_target::SshTarget, FlUrlError};

use hyper::client::conn::http1::SendRequest;

const BUFFER_SIZE: usize = 512 * 1024;

pub async fn connect_to_http_over_ssh(
    ssh_target: &SshTarget,
    remote_host: &str,
    remote_port: u16,
    time_out: Duration,
) -> Result<(Arc<SshSession>, SendRequest<Full<Bytes>>), FlUrlError> {
    let ssh_session = Arc::new(SshSession::new(ssh_target.credentials.clone()));

    let (host, port) = ssh_target.credentials.get_host_port();
    let ssh_channel = ssh_session
        .connect_to_remote_host(remote_host, remote_port, time_out)
        .await?;

    let buf_writer = tokio::io::BufWriter::with_capacity(
        BUFFER_SIZE,
        tokio::io::BufReader::with_capacity(BUFFER_SIZE, ssh_channel),
    );

    let io = TokioIo::new(buf_writer);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    let proxy_pass_uri = format!("{}:{}", host, port);

    tokio::task::spawn(async move {
        if let Err(err) = conn.with_upgrades().await {
            println!(
                "Http Connection to http://{} is failed: {:?}",
                proxy_pass_uri, err
            );
        }

        //Here
    });

    sender.ready().await?;

    Ok((ssh_session, sender))
}
