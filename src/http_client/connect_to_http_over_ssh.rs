use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use my_ssh::SshSession;

use crate::FlUrlError;

use hyper::client::conn::http1::SendRequest;

pub async fn connect_to_http_over_ssh(
    ssh_credentials: &Arc<my_ssh::SshCredentials>,
    ssh_sessions_pool: Option<&Arc<my_ssh::SshSessionsPool>>,
    remote_host: &str,
    remote_port: u16,
    time_out: Duration,
    buffer_size: usize,
) -> Result<(Arc<SshSession>, SendRequest<Full<Bytes>>), FlUrlError> {
    let ssh_session = if let Some(ssh_sessions_pool) = ssh_sessions_pool {
        ssh_sessions_pool.get_or_create(ssh_credentials).await
    } else {
        Arc::new(SshSession::new(ssh_credentials.clone()))
    };

    println!(
        "Connecting to remote host: {}:{} over SSH",
        remote_host, remote_port
    );

    let (host, port) = ssh_session.get_ssh_credentials().get_host_port();
    let ssh_channel = ssh_session
        .connect_to_remote_host(remote_host, remote_port, time_out)
        .await?;

    let buf_writer = tokio::io::BufWriter::with_capacity(
        buffer_size,
        tokio::io::BufReader::with_capacity(buffer_size, ssh_channel),
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
