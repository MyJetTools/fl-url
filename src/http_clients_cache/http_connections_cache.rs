use std::{collections::HashMap, sync::Arc};

use my_http_client::MyHttpClientConnector;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use tokio::{net::TcpStream, sync::Mutex};

use my_tls::tokio_rustls::client::TlsStream;

use crate::{http_connectors::*, my_http_client_wrapper::MyHttpClientWrapper, ConnectionParams};

pub struct ConnectionItem<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
> {
    pub last_update: DateTimeAsMicroseconds,
    pub connection: Arc<MyHttpClientWrapper<TStream, TConnector>>,
}

pub struct FlUrlHttpConnectionsCacheInner {
    max_connections: usize,
    http: HashMap<String, Vec<ConnectionItem<TcpStream, HttpConnector>>>,
    https: HashMap<String, Vec<ConnectionItem<TlsStream<TcpStream>, HttpsConnector>>>,
    #[cfg(unix)]
    unix_socket: HashMap<String, Vec<ConnectionItem<UnixSocketStream, UnixSocketConnector>>>,
    #[cfg(feature = "with-ssh")]
    ssh: HashMap<String, Vec<ConnectionItem<my_ssh::SshAsyncChannel, SshHttpConnector>>>,
}

impl Default for FlUrlHttpConnectionsCacheInner {
    fn default() -> Self {
        Self {
            max_connections: 5,
            http: Default::default(),
            https: Default::default(),
            unix_socket: Default::default(),
            #[cfg(feature = "with-ssh")]
            ssh: Default::default(),
        }
    }
}

pub struct FlUrlHttpConnectionsCache {
    pub inner: Mutex<FlUrlHttpConnectionsCacheInner>,
}

impl FlUrlHttpConnectionsCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(FlUrlHttpConnectionsCacheInner::default()),
        }
    }

    pub async fn get_http_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<TcpStream, HttpConnector>> {
        //let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into())

        let connection_key = super::utils::get_http_connection_key(params.remote_endpoint);

        let mut write_access = self.inner.lock().await;

        get_connection(
            &mut write_access.http,
            connection_key.as_str(),
            params.reuse_connection_timeout_seconds,
            || {
                super::creators::HttpConnectionCreator::create_connection(
                    params,
                    connection_key.to_string(),
                )
            },
        )
    }

    pub async fn put_http_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<TcpStream, HttpConnector>>,
    ) {
        let mut write_access = self.inner.lock().await;
        let max_connections = write_access.max_connections;
        put_connection_back(&mut write_access.http, max_connections, connection);
    }

    pub async fn get_https_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>> {
        //let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into())

        let connection_key = super::utils::get_http_connection_key(params.remote_endpoint);

        let mut write_access = self.inner.lock().await;

        get_connection(
            &mut write_access.https,
            connection_key.as_str(),
            params.reuse_connection_timeout_seconds,
            || {
                super::creators::HttpsConnectionCreator::create_connection(
                    params,
                    connection_key.to_string(),
                )
            },
        )
    }

    pub async fn put_https_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<TlsStream<TcpStream>, HttpsConnector>>,
    ) {
        let mut write_access = self.inner.lock().await;
        let max_connections = write_access.max_connections;
        put_connection_back(&mut write_access.https, max_connections, connection);
    }

    #[cfg(feature = "with-ssh")]
    pub async fn get_ssh_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>> {
        let Some(ssh_session) = params.ssh_session.clone() else {
            panic!("ssh_credentials is none");
        };

        let connection_key = super::utils::get_ssh_connection_key(
            ssh_session.get_ssh_credentials(),
            params.remote_endpoint,
        );

        let mut write_access = self.inner.lock().await;

        get_connection(
            &mut write_access.ssh,
            connection_key.as_str(),
            params.reuse_connection_timeout_seconds,
            || {
                super::creators::SshConnectionCreator::create_connection(
                    params,
                    connection_key.to_string(),
                )
            },
        )
    }

    #[cfg(feature = "with-ssh")]
    pub async fn put_ssh_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<my_ssh::SshAsyncChannel, SshHttpConnector>>,
    ) {
        let mut write_access = self.inner.lock().await;
        let max_connections = write_access.max_connections;
        put_connection_back(&mut write_access.ssh, max_connections, connection);
    }

    #[cfg(unix)]
    pub async fn get_unix_socket_connection(
        &self,
        params: &ConnectionParams<'_>,
    ) -> Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>> {
        //let remote_endpoint = url_builder.get_remote_endpoint(HTTP_DEFAULT_PORT.into())

        let connection_key = super::utils::get_unix_socket_connection_key(params.remote_endpoint);

        let mut write_access = self.inner.lock().await;

        get_connection(
            &mut write_access.unix_socket,
            connection_key.as_str(),
            params.reuse_connection_timeout_seconds,
            || {
                super::creators::UnixSocketHttpClientCreator::create_connection(
                    params,
                    connection_key.to_string(),
                )
            },
        )
    }

    #[cfg(unix)]
    pub async fn put_unix_socket_connection_back(
        &self,
        connection: Arc<MyHttpClientWrapper<UnixSocketStream, UnixSocketConnector>>,
    ) {
        let mut write_access = self.inner.lock().await;
        let max_connections = write_access.max_connections;
        put_connection_back(&mut write_access.unix_socket, max_connections, connection);
    }
}

fn get_connection<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>(
    connections: &mut HashMap<String, Vec<ConnectionItem<TStream, TConnector>>>,
    hash_map_key: &str,
    connection_timeout_seconds: i64,
    create_connection: impl Fn() -> Arc<MyHttpClientWrapper<TStream, TConnector>>,
) -> Arc<MyHttpClientWrapper<TStream, TConnector>> {
    let now = DateTimeAsMicroseconds::now();

    if let Some(http_connections) = connections.get_mut(hash_map_key) {
        while http_connections.len() > 0 {
            let result = http_connections.remove(0);

            if now.duration_since(result.last_update).get_full_seconds()
                < connection_timeout_seconds
            {
                return result.connection;
            }
        }
    }

    let new_one = create_connection();

    connections.insert(
        hash_map_key.to_string(),
        vec![ConnectionItem {
            last_update: DateTimeAsMicroseconds::now(),
            connection: new_one.clone(),
        }],
    );

    new_one
}

fn put_connection_back<
    TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    TConnector: MyHttpClientConnector<TStream> + Send + Sync + 'static,
>(
    connections: &mut HashMap<String, Vec<ConnectionItem<TStream, TConnector>>>,
    max_connections: usize,
    connection: Arc<MyHttpClientWrapper<TStream, TConnector>>,
) {
    let item = ConnectionItem {
        last_update: DateTimeAsMicroseconds::now(),
        connection,
    };
    match connections.get_mut(&item.connection.key) {
        Some(connections) => {
            if connections.len() < max_connections {
                connections.push(item);
            }
        }
        None => {
            connections.insert(item.connection.key.to_string(), vec![item]);
        }
    }
}
