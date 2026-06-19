use axum::extract::ws::Message;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct ConnectionRegistry {
    connections: Arc<DashMap<String, mpsc::Sender<Message>>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self { connections: Arc::new(DashMap::new()) }
    }

    pub fn add(&self, user_id: String, tx: mpsc::Sender<Message>) {
        debug!(user_id = %user_id, "connection registered");
        self.connections.insert(user_id, tx);
    }

    pub fn remove(&self, user_id: &str) {
        debug!(user_id = %user_id, "connection removed");
        self.connections.remove(user_id);
    }

    pub async fn broadcast(&self, audience: &[String], payload: Vec<u8>) {
        let msg = Message::Text(String::from_utf8_lossy(&payload).into_owned().into());
        for user_id in audience {
            if let Some(tx) = self.connections.get(user_id) {
                if tx.send(msg.clone()).await.is_err() {
                    warn!(user_id = %user_id, "connection closed mid-broadcast, removing");
                    drop(tx);
                    self.connections.remove(user_id);
                }
            }
        }
    }
}
