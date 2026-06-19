# game-gateway

WebSocket multiplayer game server (Tokio + Axum). State persisted in Redis via
MessagePack compatible with Go's `tinylib/msgp`.

## Quick start

```sh
# Redis must be running on localhost:6379 (or set REDIS_URL)
cargo run
```

Server listens on `0.0.0.0:${PORT}` (default `8080`) and exposes:
- `GET /ws` — WebSocket upgrade (requires a `userId` or `session` cookie)
- `GET /health` — returns `"ok"`

## Commands

| Command | Purpose |
|---|---|
| `cargo test` | All unit tests (domain logic + msgpack codec) |
| `cargo test <name>` | Focused test |
| `cargo run --example smoke_test` | Integration test — requires running server + Redis |

## Architecture

```
src/
├── main.rs             — Axum router, graceful shutdown, actor drain
├── config.rs           — Env-based config (REDIS_URL, PORT)
├── socket/
│   ├── handler.rs      — WS upgrade → per-connection inbound/outbound tasks
│   └── registry.rs     — DashMap<userId, mpsc::Sender> for broadcast
├── session/mod.rs      — Auth: reads userId or session cookie (mock)
├── game/
│   ├── registry.rs     — DashMap<gameId, mpsc::Sender<Command>>, lazy-start actors
│   ├── actor.rs        — Per-game event loop: load → apply → persist → broadcast
│   ├── store.rs        — Redis read/write via msgpack (key: `game:{id}`)
│   └── domain/         — Pure domain: Command enum (JSON), GameEvent enum (broadcast)
└── codec/msgp.rs       — msgpack (rmpv) codec matching Go's tinylib/msgp wire format
```

## Key facts

- **Auth is mocked**: reads `userId` cookie from the WS upgrade headers. Falls back to `session` cookie.
- **Commands**: JSON with `"command"` tag discriminator (`JoinPlayer`, `StartGame`, `RegisterAnswer`, `NextQuestion`).
- **Events**: JSON envelope with `"type"` field, broadcast to `audience` via `ConnectionRegistry`.
- **Persistence**: Only `StartGame` and `NextQuestion` trigger a Redis save. Other commands mutate in-memory state only.
- **Shutdown**: Ctrl+C / SIGTERM → listener stops → `GameRegistry::drain` waits for actors (30s timeout).
- **No lint/format config** in repo — uses `cargo` defaults.
- **No CI** configured.
