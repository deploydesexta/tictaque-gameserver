use serde::Serialize;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::socket::registry::ConnectionRegistry;

use super::domain::{commands::Command, events::GameEvent, game::STATUS_FINISHED};
use super::{registry::GameRegistry, store};

/// Entry point spawned by the registry. Always calls `registry.remove` exactly once on exit.
pub(super) async fn run(
    game_id: String,
    mut rx: mpsc::Receiver<Command>,
    connections: ConnectionRegistry,
    redis: redis::Client,
    registry: GameRegistry,
) {
    let result = run_game(&game_id, &mut rx, &connections, &redis).await;
    registry.remove(&game_id);
    match result {
        Ok(()) => info!(game_id = %game_id, "game actor terminated"),
        Err(e) => error!(game_id = %game_id, error = %e, "game actor failed"),
    }
}

async fn run_game(
    game_id: &str,
    rx: &mut mpsc::Receiver<Command>,
    connections: &ConnectionRegistry,
    redis: &redis::Client,
) -> anyhow::Result<()> {
    let mut game = store::load(redis, game_id).await?;

    while let Some(cmd) = rx.recv().await {
        let should_dump = matches!(
            &cmd,
            Command::StartGame { .. } | Command::NextQuestion { .. }
        );

        if let Err(e) = game.apply_command(&cmd) {
            warn!(game_id = %game_id, error = %e, "command rejected by domain");
            continue;
        }

        if should_dump {
            if let Err(e) = store::save(redis, &game).await {
                error!(game_id = %game_id, error = %e, "failed to persist game state");
            }
        }

        let events = game.take_events();
        broadcast(events, connections).await;

        if game.status == STATUS_FINISHED {
            break;
        }
    }

    Ok(())
}

// ── Broadcast ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WsEnvelope<'a> {
    id: &'a str,
    version: i32,
    #[serde(rename = "type")]
    event_type: &'a str,
    payload: serde_json::Value,
    audience: &'a [String],
}

async fn broadcast(events: Vec<GameEvent>, connections: &ConnectionRegistry) {
    for ev in events {
        let envelope = WsEnvelope {
            id: "",
            version: 1,
            event_type: ev.event_type(),
            payload: ev.payload(),
            audience: ev.audience(),
        };

        let json = match serde_json::to_vec(&envelope) {
            Ok(b) => b,
            Err(e) => {
                error!("failed to serialize event envelope: {}", e);
                continue;
            }
        };

        connections.broadcast(ev.audience(), json).await;
    }
}
