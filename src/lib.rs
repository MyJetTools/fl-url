//! # FlUrl
//!
//! A fluent, async HTTP client. The **same public API** compiles for two very
//! different transports, selected automatically by the target:
//!
//! * **native** (`cfg(not(target_arch = "wasm32"))`) — the full hyper/tokio
//!   backend in [`mod@non_wasm`]: HTTP/1.1 & HTTP/2, TLS + client certificates,
//!   connection pooling, unix sockets and (on unix) SSH tunneling.
//! * **wasm32** (`cfg(target_arch = "wasm32")`) — the browser `fetch` backend in
//!   [`mod@wasm`]. Connection pooling, TLS and redirects are handled by the
//!   browser, so those knobs become no-ops; every request-building and
//!   response-reading method keeps its native signature.
//!
//! Both backends alias their `FlUrl`, `FlUrlResponse`, `FlUrlHeaders`, … to this
//! crate root, so `flurl::FlUrl` resolves to whichever backend is active and
//! call sites need no `cfg` of their own.
//!
//! The shared, transport-agnostic pieces — [`enum@FlUrlError`], the request
//! [`body`] types and the drop-connection scenario — live at the crate root and
//! are used by both backends.

// ---- Shared, target-agnostic modules ---------------------------------------

pub mod body;
mod errors;
mod fl_drop_connection_scenario;

pub use errors::*;
pub use fl_drop_connection_scenario::*;

pub extern crate my_http_utils;

#[cfg(not(target_arch = "wasm32"))]
mod consts;

// ---- Native backend --------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod non_wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use non_wasm::*;

// ---- wasm backend ----------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
