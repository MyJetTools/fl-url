use std::sync::Arc;

use my_ssh::{SshCredentials, SshSession};
use tokio::sync::Mutex;

pub struct SshTarget {
    pub credentials: Option<Arc<SshCredentials>>,
    pub session_cache: Option<Arc<FlUrlSshSessionsCache>>,
}

pub struct FlUrlSshSessionsCache {
    sessions: Mutex<Vec<Arc<SshSession>>>,
}

impl FlUrlSshSessionsCache {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(Vec::new()),
        }
    }

    pub async fn get(&self, ssh_credentials: &SshCredentials) -> Option<Arc<SshSession>> {
        let sessions = self.sessions.lock().await;
        for session in sessions.iter() {
            if session.get_ssh_credentials().are_same(ssh_credentials) {
                return Some(session.clone());
            }
        }
        None
    }

    pub async fn insert(&self, ssh_session: &Arc<SshSession>) {
        let mut sessions = self.sessions.lock().await;

        sessions.retain(|session| {
            session
                .get_ssh_credentials()
                .are_same(ssh_session.get_ssh_credentials())
        });
        sessions.push(ssh_session.clone());

        println!("Inserted Session. Sessions in cache: {}", sessions.len());
    }
}
