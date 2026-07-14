//! The wasm32 backend of FlUrl, built on top of the browser `fetch` API.
//!
//! Everything here mirrors the public surface of the native ([`crate::non_wasm`])
//! backend so that code written against `flurl::FlUrl` compiles unchanged for
//! both targets. The shared, transport-agnostic pieces (`FlUrlError`, the request
//! `body` types, the drop-connection scenario) live at the crate root and are
//! used as-is.
//!
//! Browser-managed concerns — connection pooling, TLS, redirects, transparent
//! response gzip — are handled by `fetch`, so the corresponding native knobs
//! (`set_connections_cache`, `accept_invalid_certificate`, `update_mode`, …) are
//! kept for signature parity but become no-ops.

mod connections_cache;
mod fetch;
mod fl_response;
mod fl_url;
mod fl_url_headers;
mod into_fl_url;

pub use connections_cache::*;
pub use fl_response::*;
pub use fl_url::*;
pub use fl_url_headers::*;
pub use into_fl_url::*;
