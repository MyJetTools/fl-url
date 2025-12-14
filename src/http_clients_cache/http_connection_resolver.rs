use std::sync::Arc;

use my_http_client::MyHttpClientConnector;
use my_tls::ClientCertificate;
use url_utils::UrlBuilder;

use crate::{fl_url::FlUrlMode, my_http_client_wrapper::MyHttpClientWrapper};

#[async_trait::async_trait]
pub trait HttpConnectionResolver<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>
{
    async fn get_http_client(
        &self,
        mode: FlUrlMode,
        url_builder: &UrlBuilder,
        host_header: Option<&str>,
        client_certificate: Option<&ClientCertificate>,

        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClientWrapper<TStream, TConnector>>;

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    );
}
