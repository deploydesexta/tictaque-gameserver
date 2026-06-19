//! Smoke test: connects to the Rust game-gateway as a WebSocket client,
//! sends a JoinPlayer command against the pre-existing game:eq0-245,
//! and asserts that a PlayerJoined event is received back.

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let game_id = std::env::args().nth(1).unwrap_or_else(|| "eq0-245".to_string());
    let player_id = "player-rust-1";
    let player_name = "Rust Tester";

    let url = format!("ws://127.0.0.1:8080/ws");
    let request = http::Request::builder()
        .uri(&url)
        .header("Host", "127.0.0.1:8080")
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("Sec-WebSocket-Version", "13")
        // Mock auth: userId cookie = player_id, which must match audience in JoinPlayer event
        .header("Cookie", format!("userId={}", player_id))
        .body(())?;

    let (mut ws, _) = connect_async(request).await?;
    println!("✓ WebSocket connected");

    let cmd = serde_json::json!({
        "command": "JoinPlayer",
        "game_id": game_id,
        "player_id": player_id,
        "player_name": player_name,
        "version": 1,
    });
    let cmd_text = cmd.to_string();
    println!("→ {}", cmd_text);
    ws.send(Message::Text(cmd_text.into())).await?;

    // Read until we get a PlayerJoined or timeout
    let timeout = tokio::time::Duration::from_secs(5);
    match tokio::time::timeout(timeout, ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            println!("← {}", text);
            let envelope: serde_json::Value = serde_json::from_str(&text)?;
            let ev_type = envelope["type"].as_str().unwrap_or("");
            let payload_game_id = envelope["payload"]["game_id"].as_str().unwrap_or("");
            let payload_player_id = envelope["payload"]["player_id"].as_str().unwrap_or("");

            assert_eq!(ev_type, "PlayerJoined", "expected PlayerJoined, got {}", ev_type);
            assert_eq!(payload_game_id, game_id, "wrong game_id in payload");
            assert_eq!(payload_player_id, player_id, "wrong player_id in payload");

            println!("\n✅  PASS — PlayerJoined received for game:{}", game_id);
        }
        Ok(Some(Ok(other))) => anyhow::bail!("unexpected message type: {:?}", other),
        Ok(Some(Err(e))) => anyhow::bail!("WS error: {}", e),
        Ok(None) => anyhow::bail!("connection closed without message"),
        Err(_) => anyhow::bail!("timeout: no message received within 5s"),
    }

    Ok(())
}
