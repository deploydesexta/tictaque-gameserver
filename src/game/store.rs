use redis::AsyncCommands;
use tracing::info;

use crate::{
    codec::msgp::{decode_game, encode_game},
    game::domain::game::Game,
};

pub async fn load(redis: &redis::Client, game_id: &str) -> anyhow::Result<Game> {
    let mut conn = redis.get_multiplexed_async_connection().await?;
    let key = format!("game:{}", game_id);
    let bytes: Option<Vec<u8>> = conn.get(&key).await?;
    let bytes = bytes.ok_or_else(|| anyhow::anyhow!("game {} not found in Redis", game_id))?;
    decode_game(&bytes)
}

pub async fn save(redis: &redis::Client, game: &Game) -> anyhow::Result<()> {
    let mut conn = redis.get_multiplexed_async_connection().await?;
    let key = format!("game:{}", game.id);
    let encoded = encode_game(game)?;
    conn.set::<_, _, ()>(&key, encoded).await?;
    info!(game_id = %game.id, status = %game.status, "game state persisted");
    Ok(())
}

pub async fn exists(redis: &redis::Client, game_id: &str) -> anyhow::Result<bool> {
    let mut conn = redis.get_multiplexed_async_connection().await?;
    let key = format!("game:{}", game_id);
    Ok(conn.exists(&key).await?)
}
