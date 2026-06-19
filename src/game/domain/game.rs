use std::collections::HashMap;
use chrono::{DateTime, Utc};
use super::{
    commands::Command,
    events::{GameEvent, Leaderboard},
    player::Player,
    question::Question,
};

pub const STATUS_LOBBY: &str = "lobby";
pub const STATUS_PLAYING: &str = "playing";
pub const STATUS_FINISHED: &str = "finished";

#[derive(Debug, Clone)]
pub struct Game {
    pub id: String,
    pub owner_id: String,
    pub quiz_id: String,
    pub status: String,
    pub questions: Vec<Question>,
    pub current_question: i64,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub players: HashMap<String, Player>,
    pub events: Vec<GameEvent>,
}

impl Game {
    pub fn apply_command(&mut self, cmd: &Command) -> anyhow::Result<()> {
        match cmd {
            Command::JoinPlayer { player_id, player_name, .. } => {
                self.join_player(player_id.clone(), player_name.clone());
                Ok(())
            }
            Command::StartGame { .. } => self.start(),
            Command::RegisterAnswer { player_id, answer_index, time_took, .. } => {
                self.register_answer(player_id, *answer_index, *time_took);
                Ok(())
            }
            Command::NextQuestion { .. } => {
                self.next_question();
                Ok(())
            }
        }
    }

    pub fn join_player(&mut self, id: String, name: String) {
        if !self.players.contains_key(&id) {
            self.players.insert(id.clone(), Player::new(id.clone(), name.clone()));
        }
        let ev = GameEvent::PlayerJoined {
            game_id: self.id.clone(),
            player_id: id.clone(),
            player_name: name,
            audience: vec![self.owner_id.clone(), id],
        };
        self.events.push(ev);
        self.updated_at = Utc::now();
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.players.is_empty() {
            anyhow::bail!("must provide at least one player");
        }
        if self.status != STATUS_LOBBY {
            anyhow::bail!("cannot start a game that has already started");
        }
        let now = Utc::now();
        self.status = STATUS_PLAYING.to_string();
        self.started_at = Some(now);
        let ev = GameEvent::GameStarted {
            game_id: self.id.clone(),
            audience: self.participants(),
        };
        self.events.push(ev);
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn next_question(&mut self) {
        let ev = if self.current_question == (self.questions.len() as i64) - 1 {
            self.status = STATUS_FINISHED.to_string();
            let now = Utc::now();
            self.finished_at = Some(now);
            GameEvent::GameFinished {
                game_id: self.id.clone(),
                leaderboard: self.leaderboard(),
                audience: self.participants(),
            }
        } else {
            self.current_question += 1;
            GameEvent::QuestionUpdated {
                game_id: self.id.clone(),
                current_question: self.current_question,
                audience: self.participants(),
            }
        };
        self.events.push(ev);
        self.updated_at = Utc::now();
    }

    pub fn register_answer(&mut self, player_id: &str, answer_index: i64, time_took: i64) {
        let idx = self.current_question as usize;
        if let Some(q) = self.questions.get_mut(idx) {
            q.register_answer(player_id.to_string(), answer_index, time_took);
        }
        let ev = GameEvent::AnswerRegistered {
            game_id: self.id.clone(),
            player_id: player_id.to_string(),
            audience: vec![self.owner_id.clone()],
        };
        self.events.push(ev);
        self.updated_at = Utc::now();
    }

    pub fn leaderboard(&self) -> Leaderboard {
        struct Score {
            name: String,
            score: i64,
        }

        let mut scores: HashMap<String, Score> = self
            .players
            .values()
            .map(|p| (p.id.clone(), Score { name: p.name.clone(), score: 0 }))
            .collect();

        for q in &self.questions {
            for answer in q.answers.values() {
                if answer.answer_index == q.correct_answer {
                    if let Some(s) = scores.get_mut(&answer.player_id) {
                        let bonus = (q.available_time - answer.time_to_answer).max(0);
                        s.score += 100 + bonus;
                    }
                }
            }
        }

        let mut ranked: Vec<_> = scores.into_values().collect();
        ranked.sort_by(|a, b| b.score.cmp(&a.score));

        Leaderboard {
            gold: ranked.first().map(|s| s.name.clone()).unwrap_or_default(),
            silver: ranked.get(1).map(|s| s.name.clone()).unwrap_or_default(),
            bronze: ranked.get(2).map(|s| s.name.clone()).unwrap_or_default(),
        }
    }

    pub fn participants(&self) -> Vec<String> {
        let mut ids = vec![self.owner_id.clone()];
        for p in self.players.values() {
            ids.push(p.id.clone());
        }
        ids
    }

    pub fn take_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::question::Question;

    fn make_game(num_questions: usize) -> Game {
        Game {
            id: "gam-01".into(),
            owner_id: "owner-1".into(),
            quiz_id: "quiz-1".into(),
            status: STATUS_LOBBY.into(),
            questions: (0..num_questions)
                .map(|i| Question {
                    answers: HashMap::new(),
                    available_time: 30,
                    correct_answer: 1,
                    choices: vec![format!("A{}", i), format!("B{}", i)],
                })
                .collect(),
            current_question: 0,
            started_at: None,
            updated_at: Utc::now(),
            finished_at: None,
            players: HashMap::new(),
            events: Vec::new(),
        }
    }

    #[test]
    fn join_player_adds_player_and_emits_event() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());

        assert!(g.players.contains_key("p1"));
        assert_eq!(g.players["p1"].name, "Alice");
        assert_eq!(g.events.len(), 1);
        matches!(&g.events[0], GameEvent::PlayerJoined { player_id, .. } if player_id == "p1");
    }

    #[test]
    fn join_player_twice_does_not_duplicate() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());
        g.join_player("p1".into(), "Alice".into());

        assert_eq!(g.players.len(), 1);
        // Event is still emitted both times (same as Go behaviour)
        assert_eq!(g.events.len(), 2);
    }

    #[test]
    fn start_requires_at_least_one_player() {
        let mut g = make_game(1);
        assert!(g.start().is_err());
    }

    #[test]
    fn start_transitions_to_playing() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());
        g.events.clear();

        g.start().unwrap();

        assert_eq!(g.status, STATUS_PLAYING);
        assert!(g.started_at.is_some());
        assert_eq!(g.events.len(), 1);
        assert!(matches!(&g.events[0], GameEvent::GameStarted { .. }));
    }

    #[test]
    fn start_fails_when_already_started() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());
        g.start().unwrap();
        assert!(g.start().is_err());
    }

    #[test]
    fn next_question_advances_index() {
        let mut g = make_game(3);
        g.join_player("p1".into(), "Alice".into());
        g.start().unwrap();
        g.events.clear();

        g.next_question();

        assert_eq!(g.current_question, 1);
        assert_eq!(g.status, STATUS_PLAYING);
        assert!(matches!(&g.events[0], GameEvent::QuestionUpdated { current_question: 1, .. }));
    }

    #[test]
    fn next_question_on_last_finishes_game() {
        let mut g = make_game(2);
        g.join_player("p1".into(), "Alice".into());
        g.start().unwrap();
        g.next_question(); // advances to question 1 (last)
        g.events.clear();

        g.next_question(); // should finish

        assert_eq!(g.status, STATUS_FINISHED);
        assert!(g.finished_at.is_some());
        assert!(matches!(&g.events[0], GameEvent::GameFinished { .. }));
    }

    #[test]
    fn register_answer_records_in_question() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());
        g.start().unwrap();
        g.events.clear();

        g.register_answer("p1", 1, 5);

        assert!(g.questions[0].answers.contains_key("p1"));
        assert_eq!(g.questions[0].answers["p1"].answer_index, 1);
        assert_eq!(g.questions[0].answers["p1"].time_to_answer, 5);
        assert!(matches!(&g.events[0], GameEvent::AnswerRegistered { player_id, .. } if player_id == "p1"));
    }

    #[test]
    fn leaderboard_scores_correct_answers_with_time_bonus() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());
        g.join_player("p2".into(), "Bob".into());
        g.start().unwrap();

        // p1 answers correctly in 5s (bonus = 30-5 = 25 → score = 125)
        g.register_answer("p1", 1, 5);
        // p2 answers incorrectly
        g.register_answer("p2", 0, 3);

        let lb = g.leaderboard();
        assert_eq!(lb.gold, "Alice");
        assert_eq!(lb.silver, "Bob");
        assert_eq!(lb.bronze, "");
    }

    #[test]
    fn participants_includes_owner_and_players() {
        let mut g = make_game(1);
        g.join_player("p1".into(), "Alice".into());

        let parts = g.participants();
        assert!(parts.contains(&"owner-1".to_string()));
        assert!(parts.contains(&"p1".to_string()));
        assert_eq!(parts.len(), 2);
    }
}
