use std::sync::Arc;

use my_ssh::SshCredentials;

pub struct SshTarget {
    pub credentials: Arc<SshCredentials>,
}
