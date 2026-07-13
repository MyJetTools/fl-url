use std::sync::Arc;

mod fl_drop_connection_scenario;
mod http_clients_cache;
pub use fl_drop_connection_scenario::*;

//mod fl_request;
mod fl_response;
mod fl_response_as_stream;
pub use fl_response_as_stream::*;
mod consts;
mod fl_url;
mod into_fl_url;
mod my_http_client_wrapper;

pub use fl_response::*;
pub use fl_url::{FlUrl, FlUrlMode, HttpVerb};
pub use http_clients_cache::*;
pub use into_fl_url::*;
//mod url_builder_owned;
//pub use url_builder_owned::*;
pub extern crate hyper;
pub extern crate url_utils;
mod response_body;
pub use response_body::*;

pub mod body;

mod http_connectors;

mod errors;
pub use errors::*;

pub extern crate my_tls;
mod fl_url_headers;
pub use fl_url_headers::*;

#[cfg(all(unix, feature = "with-ssh"))]
pub mod ssh;

#[cfg(all(unix, feature = "with-ssh"))]
pub extern crate my_ssh;

mod compiled_http_request;
mod escaped_body_guard;

lazy_static::lazy_static! {
    static ref CLIENTS_CACHED: Arc<FlUrlHttpConnectionsCache> =  Arc::new(FlUrlHttpConnectionsCache::new());
}

/// The process-global connection cache used by every `FlUrl` that has no
/// explicit `set_connections_cache`. Exposed so long-running services can
/// schedule `gc(ttl_seconds)` sweeps or `clear()` it on shutdown.
pub fn shared_connections_cache() -> Arc<FlUrlHttpConnectionsCache> {
    CLIENTS_CACHED.clone()
}
