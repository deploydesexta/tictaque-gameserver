//! Encode/decode Game ↔ msgpack bytes compatible with Go's tinylib/msgp.
//!
//! Key invariants:
//! - Game map: 10 fixed fields in the same order Go writes them
//! - time.Time → ext8, len=12, type=5: [8-byte BE i64 seconds][4-byte BE i32 nanos]
//! - *time.Time nil → msgpack nil (0xC0)
//! - Integer values: most compact signed representation (positive fixnum for 0-127)

use std::collections::HashMap;
use chrono::{DateTime, TimeZone, Utc};
use rmpv::Value;
use crate::game::domain::{
    game::Game,
    player::Player,
    question::{Answer, Question},
};

pub fn decode_game(data: &[u8]) -> anyhow::Result<Game> {
    let mut cur = std::io::Cursor::new(data);
    let value = rmpv::decode::read_value(&mut cur)?;

    let map = match value {
        Value::Map(m) => m,
        other => anyhow::bail!("expected msgpack map for Game, got {:?}", other),
    };

    let mut id = String::new();
    let mut owner_id = String::new();
    let mut quiz_id = String::new();
    let mut status = String::new();
    let mut questions = Vec::new();
    let mut current_question: i64 = 0;
    let mut started_at: Option<DateTime<Utc>> = None;
    let mut updated_at: DateTime<Utc> = Utc::now();
    let mut finished_at: Option<DateTime<Utc>> = None;
    let mut players: HashMap<String, Player> = HashMap::new();

    for (k, v) in map {
        let key = match &k {
            Value::String(s) => s.as_str().unwrap_or("").to_string(),
            _ => continue,
        };
        match key.as_str() {
            "id" => id = string_val(&v)?,
            "owner_id" => owner_id = string_val(&v)?,
            "quiz_id" => quiz_id = string_val(&v)?,
            "status" => status = string_val(&v)?,
            "current_question" => current_question = int_val(&v)?,
            "started_at" => started_at = opt_time_val(&v)?,
            "updated_at" => updated_at = time_val(&v)?,
            "finished_at" => finished_at = opt_time_val(&v)?,
            "questions" => questions = decode_questions(&v)?,
            "players" => players = decode_players(&v)?,
            _ => {}
        }
    }

    Ok(Game {
        id,
        owner_id,
        quiz_id,
        status,
        questions,
        current_question,
        started_at,
        updated_at,
        finished_at,
        players,
        events: Vec::new(),
    })
}

pub fn encode_game(game: &Game) -> anyhow::Result<Vec<u8>> {
    // Build rmpv map in the EXACT same field order as Go's MarshalMsg.
    // Go's decoder accepts any order, but writing in the same order eases diff comparison.
    let map = vec![
        kv("id", Value::String(game.id.as_str().into())),
        kv("owner_id", Value::String(game.owner_id.as_str().into())),
        kv("quiz_id", Value::String(game.quiz_id.as_str().into())),
        kv("status", Value::String(game.status.as_str().into())),
        kv("questions", encode_questions(&game.questions)),
        kv("current_question", Value::Integer(game.current_question.into())),
        kv("started_at", encode_opt_time(game.started_at.as_ref())),
        kv("updated_at", encode_time(&game.updated_at)),
        kv("finished_at", encode_opt_time(game.finished_at.as_ref())),
        kv("players", encode_players(&game.players)),
    ];

    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &Value::Map(map))?;
    Ok(buf)
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn kv(key: &str, val: Value) -> (Value, Value) {
    (Value::String(key.into()), val)
}

fn string_val(v: &Value) -> anyhow::Result<String> {
    match v {
        Value::String(s) => Ok(s.as_str().unwrap_or("").to_string()),
        other => anyhow::bail!("expected string, got {:?}", other),
    }
}

fn int_val(v: &Value) -> anyhow::Result<i64> {
    match v {
        Value::Integer(i) => i.as_i64().ok_or_else(|| anyhow::anyhow!("integer out of range")),
        other => anyhow::bail!("expected integer, got {:?}", other),
    }
}

fn time_val(v: &Value) -> anyhow::Result<DateTime<Utc>> {
    match v {
        Value::Ext(5, data) if data.len() == 12 => decode_ext_time(data),
        other => anyhow::bail!("expected ext-5 time, got {:?}", other),
    }
}

fn opt_time_val(v: &Value) -> anyhow::Result<Option<DateTime<Utc>>> {
    match v {
        Value::Nil => Ok(None),
        Value::Ext(5, data) if data.len() == 12 => Ok(Some(decode_ext_time(data)?)),
        other => anyhow::bail!("expected nil or ext-5 time, got {:?}", other),
    }
}

fn decode_ext_time(data: &[u8]) -> anyhow::Result<DateTime<Utc>> {
    let seconds = i64::from_be_bytes(data[0..8].try_into()?);
    let nanos = i32::from_be_bytes(data[8..12].try_into()?);
    Utc.timestamp_opt(seconds, nanos as u32)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid timestamp secs={} nanos={}", seconds, nanos))
}

fn encode_time(dt: &DateTime<Utc>) -> Value {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&dt.timestamp().to_be_bytes());
    data.extend_from_slice(&(dt.timestamp_subsec_nanos() as i32).to_be_bytes());
    Value::Ext(5, data)
}

fn encode_opt_time(dt: Option<&DateTime<Utc>>) -> Value {
    match dt {
        None => Value::Nil,
        Some(t) => encode_time(t),
    }
}

fn decode_questions(v: &Value) -> anyhow::Result<Vec<Question>> {
    let arr = match v {
        Value::Array(a) => a,
        other => anyhow::bail!("expected array for questions, got {:?}", other),
    };
    arr.iter().map(decode_question).collect()
}

fn decode_question(v: &Value) -> anyhow::Result<Question> {
    let map = match v {
        Value::Map(m) => m,
        other => anyhow::bail!("expected map for question, got {:?}", other),
    };

    let mut answers: HashMap<String, Answer> = HashMap::new();
    let mut available_time: i64 = 0;
    let mut correct_answer: i64 = 0;
    let mut choices: Vec<String> = Vec::new();

    for (k, v) in map {
        let key = match k {
            Value::String(s) => s.as_str().unwrap_or("").to_string(),
            _ => continue,
        };
        match key.as_str() {
            "answers" => answers = decode_answer_map(v)?,
            "available_time" => available_time = int_val(v)?,
            "correct_answer" => correct_answer = int_val(v)?,
            "choices" => choices = decode_string_array(v)?,
            _ => {}
        }
    }

    Ok(Question { answers, available_time, correct_answer, choices })
}

fn decode_answer_map(v: &Value) -> anyhow::Result<HashMap<String, Answer>> {
    let map = match v {
        Value::Map(m) => m,
        other => anyhow::bail!("expected map for answers, got {:?}", other),
    };

    let mut out = HashMap::new();
    for (k, v) in map {
        let player_id = string_val(k)?;
        let answer = decode_answer(v)?;
        out.insert(player_id, answer);
    }
    Ok(out)
}

fn decode_answer(v: &Value) -> anyhow::Result<Answer> {
    let map = match v {
        Value::Map(m) => m,
        other => anyhow::bail!("expected map for answer, got {:?}", other),
    };

    let mut player_id = String::new();
    let mut answer_index: i64 = 0;
    let mut time_to_answer: i64 = 0;

    for (k, v) in map {
        let key = match k {
            Value::String(s) => s.as_str().unwrap_or("").to_string(),
            _ => continue,
        };
        match key.as_str() {
            "player_id" => player_id = string_val(v)?,
            "answer_index" => answer_index = int_val(v)?,
            "time_to_answer" => time_to_answer = int_val(v)?,
            _ => {}
        }
    }
    Ok(Answer { player_id, answer_index, time_to_answer })
}

fn decode_string_array(v: &Value) -> anyhow::Result<Vec<String>> {
    let arr = match v {
        Value::Array(a) => a,
        other => anyhow::bail!("expected array for choices, got {:?}", other),
    };
    arr.iter().map(string_val).collect()
}

fn decode_players(v: &Value) -> anyhow::Result<HashMap<String, Player>> {
    let map = match v {
        Value::Map(m) => m,
        other => anyhow::bail!("expected map for players, got {:?}", other),
    };
    let mut out = HashMap::new();
    for (k, v) in map {
        let player_id = string_val(k)?;
        let player = decode_player(v)?;
        out.insert(player_id, player);
    }
    Ok(out)
}

fn decode_player(v: &Value) -> anyhow::Result<Player> {
    let map = match v {
        Value::Map(m) => m,
        other => anyhow::bail!("expected map for player, got {:?}", other),
    };
    let mut id = String::new();
    let mut name = String::new();
    for (k, v) in map {
        let key = match k {
            Value::String(s) => s.as_str().unwrap_or("").to_string(),
            _ => continue,
        };
        match key.as_str() {
            "id" => id = string_val(v)?,
            "name" => name = string_val(v)?,
            _ => {}
        }
    }
    Ok(Player { id, name })
}

fn encode_questions(questions: &[Question]) -> Value {
    Value::Array(questions.iter().map(encode_question).collect())
}

fn encode_question(q: &Question) -> Value {
    Value::Map(vec![
        kv("answers", encode_answer_map(&q.answers)),
        kv("available_time", Value::Integer(q.available_time.into())),
        kv("correct_answer", Value::Integer(q.correct_answer.into())),
        kv("choices", Value::Array(q.choices.iter().map(|s| Value::String(s.as_str().into())).collect())),
    ])
}

fn encode_answer_map(answers: &HashMap<String, Answer>) -> Value {
    Value::Map(
        answers
            .iter()
            .map(|(k, a)| {
                (
                    Value::String(k.as_str().into()),
                    Value::Map(vec![
                        kv("player_id", Value::String(a.player_id.as_str().into())),
                        kv("answer_index", Value::Integer(a.answer_index.into())),
                        kv("time_to_answer", Value::Integer(a.time_to_answer.into())),
                    ]),
                )
            })
            .collect(),
    )
}

fn encode_players(players: &HashMap<String, Player>) -> Value {
    Value::Map(
        players
            .iter()
            .map(|(k, p)| {
                (
                    Value::String(k.as_str().into()),
                    Value::Map(vec![
                        kv("id", Value::String(p.id.as_str().into())),
                        kv("name", Value::String(p.name.as_str().into())),
                    ]),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn roundtrip_game() {
        let now = Utc::now().with_nanosecond(123_456_789).unwrap();
        let game = Game {
            id: "abc-def".into(),
            owner_id: "user-1".into(),
            quiz_id: "quiz-1".into(),
            status: "lobby".into(),
            questions: vec![Question {
                answers: HashMap::new(),
                available_time: 30,
                correct_answer: 1,
                choices: vec!["A".into(), "B".into(), "C".into()],
            }],
            current_question: 0,
            started_at: None,
            updated_at: now,
            finished_at: None,
            players: HashMap::new(),
            events: Vec::new(),
        };

        let encoded = encode_game(&game).expect("encode failed");
        let decoded = decode_game(&encoded).expect("decode failed");

        assert_eq!(decoded.id, game.id);
        assert_eq!(decoded.owner_id, game.owner_id);
        assert_eq!(decoded.status, game.status);
        assert_eq!(decoded.current_question, game.current_question);
        assert_eq!(decoded.questions.len(), 1);
        assert_eq!(decoded.questions[0].available_time, 30);
        assert_eq!(decoded.questions[0].correct_answer, 1);
        assert_eq!(decoded.questions[0].choices, vec!["A", "B", "C"]);
        assert_eq!(decoded.updated_at.timestamp(), now.timestamp());
        assert_eq!(decoded.updated_at.timestamp_subsec_nanos(), now.timestamp_subsec_nanos());
    }

    #[test]
    fn time_nil_roundtrip() {
        let now = Utc::now();
        let game = Game {
            id: "x".into(),
            owner_id: "o".into(),
            quiz_id: "q".into(),
            status: "lobby".into(),
            questions: Vec::new(),
            current_question: 0,
            started_at: Some(now),
            updated_at: now,
            finished_at: None,
            players: HashMap::new(),
            events: Vec::new(),
        };

        let encoded = encode_game(&game).unwrap();
        let decoded = decode_game(&encoded).unwrap();

        assert!(decoded.started_at.is_some());
        assert!(decoded.finished_at.is_none());
        assert_eq!(decoded.started_at.unwrap().timestamp(), now.timestamp());
    }

    /// Decode the real bytes produced by Go's msgp encoder (captured from live Redis).
    /// Verifies that our decoder handles the actual wire format correctly.
    #[test]
    fn decode_real_go_bytes() {
        // Captured: docker exec redis redis-cli --raw get "game:eq0-245" | xxd -p | tr -d '\n'
        let hex = "8aa26964a76571302d323435a86f776e65725f6964ac6c6f316d3562706d676a\
                   6b66a77175697a5f6964ac6c736a6635786d7439643968a6737461747573a56c\
                   6f626279a97175657374696f6e739384a7616e737765727380ae617661696c61\
                   626c655f74696d650fae636f72726563745f616e7377657202a763686f696365\
                   7394aa53c3a36f205061756c6fae52696f206465204a616e6569726fa9427261\
                   73c3ad6c6961a853616c7661646f7284a7616e737765727380ae617661696c61\
                   626c655f74696d650aae636f72726563745f616e7377657200a763686f696365\
                   7394a55061726973a44c796f6ea84d617273656c6861a44e69636584a7616e73\
                   7765727380ae617661696c61626c655f74696d6514ae636f72726563745f616e\
                   7377657202a763686f6963657394a45365756ca650657175696da754c3b37175\
                   696fab42616e677565636f717565b063757272656e745f7175657374696f6e00\
                   aa737461727465645f6174c0aa757064617465645f6174c70c05000000006a34\
                   54e016520ce0ab66696e69736865645f6174c0a7706c6179657273800a";

        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();

        let game = decode_game(&bytes).expect("decode of real Go bytes failed");

        assert_eq!(game.id, "eq0-245");
        assert_eq!(game.status, "lobby");
        assert_eq!(game.questions.len(), 3);
        assert!(game.started_at.is_none());
        assert!(game.finished_at.is_none());
        assert_eq!(game.players.len(), 0);
        assert_eq!(game.current_question, 0);
        // updated_at decoded from ext-5 time must be non-zero
        assert!(game.updated_at.timestamp() > 0);
    }
}
