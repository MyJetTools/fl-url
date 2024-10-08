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
mod url_builder_owned;
pub use url_builder_owned::*;
pub extern crate hyper;
mod response_body;
pub use response_body::*;

mod http_client;
pub use http_client::*;
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
