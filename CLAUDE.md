# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Rust toolchain is installed via rustup — must be on PATH
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"

cargo build                        # dev build
cargo build --release              # release build
cargo test                         # all tests (16)
cargo test <name>                  # single test, e.g. cargo test decode_real_go_bytes
cargo run                          # start server (default: port 8080, redis://localhost:6379)
cargo run --example smoke_test     # integration test (requires running server + Redis)

# Env vars
REDIS_URL=redis://localhost:6379   # default
PORT=8080                          # default
RUST_LOG=info                      # tracing filter
```

## Architecture

WebSocket game server using the **actor model**: one Tokio task per active game, state held in memory, Redis as checkpoint store.

```
Client WS ──→ socket/handler.rs
                  │ parses Command (JSON)
                  ▼
             game/registry.rs  ←── lazy-starts one task per game_id
                  │ mpsc::Sender<Command>
                  ▼
             game/actor.rs     ←── owns Game in memory, applies commands
                  │ on StartGame / NextQuestion
                  ▼
             game/store.rs     ←── Redis GET/SET (msgpack)
                  │ events
                  ▼
             socket/registry.rs ──→ each connected userId's mpsc channel ──→ WS sink
```

**AppState** (in `main.rs`) is cloned into every request — it holds `redis::Client`, `ConnectionRegistry`, and `GameRegistry`, all cheap to clone because the inner maps are `Arc<DashMap>`.

**No DI framework** — dependencies wired manually through `AppState` and function arguments.

## Critical invariants

### msgpack compatibility (`codec/msgp.rs`)

Redis stores Game state written by Go's `tinylib/msgp`. The codec must preserve:
- Exactly **10 fields** in fixed order: `id, owner_id, quiz_id, status, questions, current_question, started_at, updated_at, finished_at, players`
- `time.Time` → msgpack ext8, type=5, 12 bytes: `[8-byte BE i64 seconds][4-byte BE i32 nanos]`
- `*time.Time` nil → msgpack nil (`0xC0`)

Breaking this format breaks `GET /games/:id` on the Go REST API. The test `decode_real_go_bytes` in `codec/msgp.rs` uses actual bytes from a live game — keep it passing.

### Actor lifecycle (`game/actor.rs`)

`run()` wraps `run_game()` and calls `registry.remove()` exactly once, regardless of exit path (load failure, game finished, or channel closed). This is what decrements `active_count` for graceful shutdown. Do not add extra `remove()` calls or the drain counter underflows.

### Persistence policy

Only `StartGame` and `NextQuestion` trigger a Redis save. Answers registered between questions live only in the actor's in-memory `Game`. If the process restarts mid-question, the current question is replayed from the last checkpoint.

## Scaling notes

State lives in one process. For multiple instances, use sticky sessions (route by `game_id`) at the load balancer. SIGTERM triggers graceful drain: axum stops accepting connections, existing actors finish, 30s timeout then force exit.
