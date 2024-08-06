use std::sync::Arc;

use my_ssh::*;

pub struct SshTarget {
    pub credentials: Option<Arc<SshCredentials>>,
    pub sessions_pool: Option<Arc<SshSessionsPool>>,
    pub http_buffer_size: usize,
}
