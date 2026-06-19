use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    pub player_id: String,
    pub answer_index: i64,
    pub time_to_answer: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub answers: HashMap<String, Answer>,
    pub available_time: i64,
    pub correct_answer: i64,
    pub choices: Vec<String>,
}

impl Question {
    pub fn register_answer(&mut self, player_id: String, answer_index: i64, time_to_answer: i64) {
        self.answers.insert(
            player_id.clone(),
            Answer { player_id, answer_index, time_to_answer },
        );
    }
}
