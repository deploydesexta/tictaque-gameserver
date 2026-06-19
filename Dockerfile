# ── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:1.86-slim AS builder

WORKDIR /app

# Cache dependencies before copying application code.
# A dummy main lets cargo compile all deps in a separate layer.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build the real application (only this layer rebuilds on source changes).
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/game-gateway .

ENV RUST_LOG=info
ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["./game-gateway"]
