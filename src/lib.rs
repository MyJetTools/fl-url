mod scheme;
use std::sync::Arc;

pub use scheme::*;

mod fl_drop_connection_scenario;
mod http_clients_cache;
pub use fl_drop_connection_scenario::*;

//mod fl_request;
mod fl_response;
mod fl_url;
mod into_fl_url;
mod url_builder;
pub mod url_utils;
pub use fl_response::*;
pub use fl_url::FlUrl;
pub use http_clients_cache::*;
pub use into_fl_url::*;
pub use url_builder::*;
//mod url_builder_owned;
//pub use url_builder_owned::*;
pub extern crate hyper;
mod response_body;
pub use response_body::*;

mod http_connectors;

mod errors;
pub use errors::*;

pub extern crate my_tls;
mod fl_url_headers;
pub use fl_url_headers::*;

#[cfg(feature = "with-ssh")]
mod ssh;
#[cfg(feature = "with-ssh")]
pub use ssh::*;
#[cfg(feature = "with-ssh")]
pub extern crate my_ssh;

lazy_static::lazy_static! {
    static ref CLIENTS_CACHED: Arc<HttpClientsCache> =  Arc::new(HttpClientsCache::new());
}

#[cfg(feature = "with-ssh")]
lazy_static::lazy_static! {
    static ref SSH_SESSIONS_POOL: Arc<my_ssh::SshSessionsPool> =  Arc::new(my_ssh::SshSessionsPool::new());
}

#[cfg(test)]
mod tests {
    use rust_extensions::StopWatch;

    use crate::FlUrl;

    #[tokio::test]
    async fn test_google_com_request() {
        let mut sw = StopWatch::new();
        sw.start();
        let mut fl_url_response = FlUrl::new("https://google.com").get().await.unwrap();

        let _ = fl_url_response.body_as_str().await.unwrap();
        println!("Status: {}", fl_url_response.get_status_code());
        sw.pause();
        println!("Elapsed: {}", sw.duration_as_string());

        let mut sw = StopWatch::new();
        sw.start();
        let mut fl_url_response = FlUrl::new("https://google.com").get().await.unwrap();

        let _ = fl_url_response.body_as_str().await.unwrap();
        println!("Status: {}", fl_url_response.get_status_code());
        sw.pause();
        println!("Elapsed: {}", sw.duration_as_string());
    }
}
