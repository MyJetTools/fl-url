//! The native (non-wasm) backend of FlUrl.
//!
//! This is the full hyper/tokio implementation — HTTP/1.1 & HTTP/2, TLS, client
//! certificates, connection pooling, unix sockets and (on unix) SSH tunneling.
//! It is compiled only for non-wasm targets; `crate::lib` aliases the types
//! defined here to the crate root so `flurl::FlUrl`, `flurl::FlUrlResponse`, …
//! resolve to this backend.

use std::sync::Arc;

mod compiled_http_request;
mod escaped_body_guard;
mod fl_response;
mod fl_response_as_stream;
mod fl_url;
mod fl_url_headers;
mod http_clients_cache;
mod http_connectors;
mod into_fl_url;
mod my_http_client_wrapper;
mod response_body;

pub use fl_response::*;
pub use fl_response_as_stream::*;
pub use fl_url::{FlUrl, FlUrlMode, HttpVerb};
pub use fl_url_headers::*;
pub use http_clients_cache::*;
pub use into_fl_url::*;
pub use response_body::*;

pub extern crate hyper;
pub extern crate my_tls;

#[cfg(all(unix, feature = "with-ssh"))]
pub mod ssh;

#[cfg(all(unix, feature = "with-ssh"))]
pub extern crate my_ssh;

lazy_static::lazy_static! {
    pub(crate) static ref CLIENTS_CACHED: Arc<FlUrlHttpConnectionsCache> =
        Arc::new(FlUrlHttpConnectionsCache::new());
}

/// The process-global connection cache used by every `FlUrl` that has no
/// explicit `set_connections_cache`. Exposed so long-running services can
/// schedule `gc(ttl_seconds)` sweeps or `clear()` it on shutdown.
pub fn shared_connections_cache() -> Arc<FlUrlHttpConnectionsCache> {
    CLIENTS_CACHED.clone()
}
