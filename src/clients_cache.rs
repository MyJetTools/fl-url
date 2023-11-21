use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::sync::RwLock;

use crate::{ClientCertificate, FlUrlError, HttpClient, UrlBuilder};

pub struct ClientsCache {
    pub clients: RwLock<HashMap<String, Arc<HttpClient>>>,
}

impl ClientsCache {
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
        {
            let read_access = self.clients.read().await;
            if read_access.contains_key(schema_and_domain.as_str()) {
                return Ok(read_access
                    .get(schema_and_domain.as_str())
                    .cloned()
                    .unwrap());
            }
        }

        let mut write_access = self.clients.write().await;

        if write_access.contains_key(schema_and_domain.as_str()) {
            return Ok(write_access
                .get(schema_and_domain.as_str())
                .cloned()
                .unwrap());
        }

        let new_one = HttpClient::new(url_builder, client_certificate, request_timeout).await?;
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
