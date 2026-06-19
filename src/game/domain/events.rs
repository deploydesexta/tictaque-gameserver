use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Leaderboard {
    pub gold: String,
    pub silver: String,
    pub bronze: String,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    PlayerJoined {
        game_id: String,
        player_id: String,
        player_name: String,
        audience: Vec<String>,
    },
    GameStarted {
        game_id: String,
        audience: Vec<String>,
    },
    QuestionUpdated {
        game_id: String,
        current_question: i64,
        audience: Vec<String>,
    },
    AnswerRegistered {
        game_id: String,
        player_id: String,
        audience: Vec<String>,
    },
    GameFinished {
        game_id: String,
        leaderboard: Leaderboard,
        audience: Vec<String>,
    },
}

impl GameEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::PlayerJoined { .. } => "PlayerJoined",
            Self::GameStarted { .. } => "GameStarted",
            Self::QuestionUpdated { .. } => "QuestionUpdated",
            Self::AnswerRegistered { .. } => "AnswerRegistered",
            Self::GameFinished { .. } => "GameFinished",
        }
    }

    pub fn audience(&self) -> &[String] {
        match self {
            Self::PlayerJoined { audience, .. } => audience,
            Self::GameStarted { audience, .. } => audience,
            Self::QuestionUpdated { audience, .. } => audience,
            Self::AnswerRegistered { audience, .. } => audience,
            Self::GameFinished { audience, .. } => audience,
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            Self::PlayerJoined { game_id, player_id, player_name, .. } => serde_json::json!({
                "game_id": game_id,
                "player_id": player_id,
                "player_name": player_name,
                "type": "PlayerJoined",
            }),
            Self::GameStarted { game_id, .. } => serde_json::json!({
                "game_id": game_id,
                "type": "GameStarted",
            }),
            Self::QuestionUpdated { game_id, current_question, .. } => serde_json::json!({
                "game_id": game_id,
                "current_question": current_question,
                "type": "QuestionUpdated",
            }),
            Self::AnswerRegistered { game_id, player_id, .. } => serde_json::json!({
                "game_id": game_id,
                "player_id": player_id,
                "type": "AnswerRegistered",
            }),
            Self::GameFinished { game_id, leaderboard, .. } => serde_json::json!({
                "game_id": game_id,
                "leaderboard": leaderboard,
                "type": "GameFinished",
            }),
        }
    }
}
