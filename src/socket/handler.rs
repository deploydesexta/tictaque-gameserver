use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    game::domain::commands::Command,
    session::extract_user_id,
    AppState,
};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = extract_user_id(&headers);
    ws.on_upgrade(move |socket| async move {
        match user_id {
            Some(id) => handle_socket(socket, id, state).await,
            None => {
                warn!("WebSocket upgrade rejected: missing userId/session cookie");
            }
        }
    })
}

async fn handle_socket(socket: WebSocket, user_id: String, state: AppState) {
    info!(user_id = %user_id, "WebSocket connected");

    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Message>(64);

    state.connections.add(user_id.clone(), tx);

    // Outbound task: drain channel → write to WebSocket.
    let outbound = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Inbound loop: read commands → dispatch to the game actor.
    while let Some(msg) = stream.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };

        debug!(user_id = %user_id, raw = %text, "received WS message");

        let cmd: Command = match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                warn!(user_id = %user_id, error = %e, "failed to parse command");
                continue;
            }
        };

        if let Err(e) = state.registry.dispatch(cmd, &state.redis_client, &state.connections).await {
            error!(user_id = %user_id, error = %e, "error dispatching command");
        }
    }

    info!(user_id = %user_id, "WebSocket disconnected");
    state.connections.remove(&user_id);
    outbound.abort();
}
