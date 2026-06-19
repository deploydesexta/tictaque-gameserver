use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "command")]
pub enum Command {
    JoinPlayer {
        game_id: String,
        player_id: String,
        player_name: String,
    },
    StartGame {
        game_id: String,
    },
    RegisterAnswer {
        game_id: String,
        player_id: String,
        answer_index: i64,
        time_took: i64,
    },
    NextQuestion {
        game_id: String,
    },
}

impl Command {
    pub fn game_id(&self) -> &str {
        match self {
            Self::JoinPlayer { game_id, .. } => game_id,
            Self::StartGame { game_id } => game_id,
            Self::RegisterAnswer { game_id, .. } => game_id,
            Self::NextQuestion { game_id } => game_id,
        }
    }
}
