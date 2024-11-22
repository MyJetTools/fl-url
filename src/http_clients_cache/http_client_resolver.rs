use std::sync::Arc;

use my_http_client::{http1::MyHttpClient, MyHttpClientConnector};
use my_tls::ClientCertificate;

use crate::UrlBuilder;

#[async_trait::async_trait]
pub trait HttpClientResolver<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>
{
    async fn get_http_client(
        &self,
        url_builder: &UrlBuilder,
        domain_override: Option<&String>,
        client_certificate: Option<&ClientCertificate>,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    ) -> Arc<MyHttpClient<TStream, TConnector>>;

    async fn drop_http_client(
        &self,
        url_builder: &UrlBuilder,
        #[cfg(feature = "with-ssh")] ssh_credentials: Option<&Arc<my_ssh::SshCredentials>>,
    );
}
