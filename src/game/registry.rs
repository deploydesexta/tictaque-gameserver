use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{mpsc, Notify};
use tracing::info;

use crate::socket::registry::ConnectionRegistry;

use super::domain::commands::Command;
use super::{actor::run, store};

#[derive(Clone)]
pub struct GameRegistry {
    actors: Arc<DashMap<String, mpsc::Sender<Command>>>,
    active_count: Arc<AtomicUsize>,
    all_done: Arc<Notify>,
}

impl GameRegistry {
    pub fn new() -> Self {
        Self {
            actors: Arc::new(DashMap::new()),
            active_count: Arc::new(AtomicUsize::new(0)),
            all_done: Arc::new(Notify::new()),
        }
    }

    pub async fn dispatch(
        &self,
        cmd: Command,
        redis: &redis::Client,
        connections: &ConnectionRegistry,
    ) -> anyhow::Result<()> {
        let game_id = cmd.game_id().to_string();
        let tx = self.get_or_start(&game_id, redis, connections).await?;

        tx.send(cmd).await.map_err(|_| {
            anyhow::anyhow!("actor channel for game {} closed unexpectedly", game_id)
        })
    }

    /// Called by the actor on exit to deregister itself.
    pub(super) fn remove(&self, game_id: &str) {
        self.actors.remove(game_id);
        // fetch_sub returns the previous value; if it was 1, count is now 0.
        let prev = self.active_count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.all_done.notify_one();
        }
        info!(game_id = %game_id, "game actor removed from registry");
    }

    /// Stops accepting new actors and waits for all running ones to finish.
    /// Called on shutdown after the WebSocket listener has stopped.
    pub async fn drain(&self, timeout: Duration) {
        // Drop all senders → actors see rx closed → their loops exit.
        self.actors.clear();

        // Register the wakeup BEFORE reading the count to avoid a race where
        // the last actor finishes and calls notify_one() between clear() and notified().
        let notified = self.all_done.notified();

        if self.active_count.load(Ordering::SeqCst) == 0 {
            return;
        }

        info!("waiting for game actors to drain (timeout: {}s)", timeout.as_secs());
        if tokio::time::timeout(timeout, notified).await.is_err() {
            let remaining = self.active_count.load(Ordering::SeqCst);
            tracing::warn!(remaining, "drain timeout reached, forcing shutdown");
        }
    }

    async fn get_or_start(
        &self,
        game_id: &str,
        redis: &redis::Client,
        connections: &ConnectionRegistry,
    ) -> anyhow::Result<mpsc::Sender<Command>> {
        if let Some(tx) = self.actors.get(game_id) {
            return Ok(tx.clone());
        }

        if !store::exists(redis, game_id).await? {
            anyhow::bail!("game {} not found", game_id);
        }

        let (tx, rx) = mpsc::channel::<Command>(32);

        let tx = match self.actors.entry(game_id.to_string()) {
            dashmap::Entry::Occupied(e) => e.get().clone(),
            dashmap::Entry::Vacant(e) => {
                let sender = e.insert(tx.clone()).clone();
                self.active_count.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(run(
                    game_id.to_string(),
                    rx,
                    connections.clone(),
                    redis.clone(),
                    self.clone(),
                ));
                info!(game_id = %game_id, "game actor started");
                sender
            }
        };

        Ok(tx)
    }
}
