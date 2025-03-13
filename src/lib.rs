use std::sync::Arc;

mod fl_drop_connection_scenario;
mod http_clients_cache;
pub use fl_drop_connection_scenario::*;

//mod fl_request;
mod fl_response;
mod fl_url;
mod into_fl_url;

pub use fl_response::*;
pub use fl_url::FlUrl;
pub use http_clients_cache::*;
pub use into_fl_url::*;
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
pub mod ssh;

#[cfg(feature = "with-ssh")]
pub extern crate my_ssh;

mod form_data_builder;
pub use form_data_builder::*;

lazy_static::lazy_static! {
    static ref CLIENTS_CACHED: Arc<HttpClientsCache> =  Arc::new(HttpClientsCache::new());
}
