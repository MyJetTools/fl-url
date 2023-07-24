use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::FlUrlClient;

pub trait FlUrlFactory {
    fn create(&mut self) -> FlUrlClient;
}

pub struct ClientsCache {
    pub clients: RwLock<HashMap<String, Arc<FlUrlClient>>>,
}

impl ClientsCache {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get(
        &self,
        schema_domain: &str,
        factory: &mut impl FlUrlFactory,
    ) -> Arc<FlUrlClient> {
        {
            let read_access = self.clients.read().await;
            if read_access.contains_key(schema_domain) {
                return read_access.get(schema_domain).cloned().unwrap();
            }
        }

        let mut write_access = self.clients.write().await;

        if write_access.contains_key(schema_domain) {
            return write_access.get(schema_domain).cloned().unwrap();
        }

        let new_one = factory.create();
        let new_one = Arc::new(new_one);

        write_access.insert(schema_domain.to_string(), new_one.clone());

        write_access.get(schema_domain).cloned().unwrap()
    }

    pub async fn remove(&self, schema_domain: &str) {
        let mut write_access = self.clients.write().await;
        write_access.remove(schema_domain);
    }
}
