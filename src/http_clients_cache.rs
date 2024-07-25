use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::sync::RwLock;

use crate::{FlUrlError, HttpClient, UrlBuilder};
use my_tls::ClientCertificate;

pub struct HttpClientsCache {
    pub clients: RwLock<HashMap<String, Arc<HttpClient>>>,
}

impl HttpClientsCache {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get(
        &self,
        url_builder: &UrlBuilder,
        request_timeout: Duration,
        client_certificate: Option<ClientCertificate>,
    ) -> Result<Arc<HttpClient>, FlUrlError> {
        let schema_and_domain = url_builder.get_scheme_and_host();

        let mut write_access = self.clients.write().await;

        if let Some(existing_connection) =
            get_existing_connection(&mut write_access, schema_and_domain.as_str())
        {
            return Ok(existing_connection);
        }

        let new_one = HttpClient::new(
            url_builder,
            client_certificate,
            request_timeout,
            #[cfg(feature = "with-ssh")]
            None,
            #[cfg(feature = "with-ssh")]
            None,
        )
        .await?;
        let new_one = Arc::new(new_one);

        write_access.insert(schema_and_domain.to_string(), new_one.clone());

        Ok(write_access
            .get(schema_and_domain.as_str())
            .cloned()
            .unwrap())
    }

    pub async fn remove(&self, schema_domain: &str) {
        let mut write_access = self.clients.write().await;
        write_access.remove(schema_domain);
    }
}

fn get_existing_connection(
    connections: &mut HashMap<String, Arc<HttpClient>>,
    schema_and_domain: &str,
) -> Option<Arc<HttpClient>> {
    let mut has_connection_disconnected = false;

    if let Some(connection) = connections.get(schema_and_domain) {
        if connection.is_disconnected() {
            has_connection_disconnected = true;
        } else {
            return Some(connection.clone());
        }
    }

    if has_connection_disconnected {
        connections.remove(schema_and_domain);
    }

    None
}
