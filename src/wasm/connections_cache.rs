use std::sync::{Arc, OnceLock};

/// wasm stub of the native `FlUrlHttpConnectionsCache`.
///
/// Under wasm the browser owns the HTTP connection pool, so there is nothing for
/// FlUrl to cache. This type carries no state; it exists only so the native
/// pooling API (`FlUrl::set_connections_cache`, [`shared_connections_cache`])
/// keeps the same signatures and portable code compiles unchanged.
pub struct FlUrlHttpConnectionsCache;

impl FlUrlHttpConnectionsCache {
    pub fn new() -> Self {
        Self
    }

    pub fn new_with_max_connections(_max_connections: usize) -> Self {
        Self
    }

    /// No-op under wasm: `fetch` owns the connection pool.
    pub fn clear(&self) {}

    /// No-op under wasm: `fetch` owns the connection pool.
    pub fn gc(&self, _reuse_connection_timeout_seconds: i64) {}
}

impl Default for FlUrlHttpConnectionsCache {
    fn default() -> Self {
        Self::new()
    }
}

static SHARED_CACHE: OnceLock<Arc<FlUrlHttpConnectionsCache>> = OnceLock::new();

/// wasm counterpart of the native `shared_connections_cache()`. Returns a shared
/// (no-op) cache handle so callers that pass it into `set_connections_cache`
/// compile and behave identically across targets.
pub fn shared_connections_cache() -> Arc<FlUrlHttpConnectionsCache> {
    SHARED_CACHE
        .get_or_init(|| Arc::new(FlUrlHttpConnectionsCache::new()))
        .clone()
}
