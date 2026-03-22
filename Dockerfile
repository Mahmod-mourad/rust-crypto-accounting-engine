# ─── Stage 1: Builder ────────────────────────────────────────────────────────
FROM rust:latest AS builder

# Install system dependencies for SQLx and OpenSSL
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY shared/observability/Cargo.toml    ./shared/observability/
COPY services/ledger-service/Cargo.toml ./services/ledger-service/
COPY services/pnl-consumer/Cargo.toml   ./services/pnl-consumer/
COPY services/pnl-grpc/Cargo.toml       ./services/pnl-grpc/
COPY services/pnl-grpc/build.rs         ./services/pnl-grpc/
COPY services/pnl-grpc/proto            ./services/pnl-grpc/proto

# Create dummy sources so Cargo can resolve the whole workspace
RUN mkdir -p shared/observability/src \
             services/ledger-service/src \
             services/ledger-service/benches \
             services/pnl-consumer/src \
             services/pnl-grpc/src/bin && \
    echo ""             > shared/observability/src/lib.rs             && \
    echo ""             > services/ledger-service/src/lib.rs         && \
    echo "fn main() {}" > services/ledger-service/src/main.rs        && \
    echo "fn main() {}" > services/ledger-service/benches/portfolio.rs && \
    echo "fn main() {}" > services/pnl-consumer/src/main.rs          && \
    echo ""             > services/pnl-grpc/src/lib.rs                && \
    echo "fn main() {}" > services/pnl-grpc/src/main.rs              && \
    echo "fn main() {}" > services/pnl-grpc/src/bin/client.rs

# Build dependencies only (cached layer)
RUN cargo build --release --package ledger-service && \
    rm -rf services/ledger-service/src services/pnl-consumer/src services/pnl-grpc/src

# Copy real application source and migrations (needed by sqlx::migrate! at compile time)
COPY shared/observability/src           ./shared/observability/src
COPY services/ledger-service/src        ./services/ledger-service/src
COPY services/ledger-service/migrations ./services/ledger-service/migrations

# Recreate dummy sources for other workspace members (deleted above)
RUN mkdir -p services/pnl-consumer/src services/pnl-grpc/src/bin && \
    echo "fn main() {}" > services/pnl-consumer/src/main.rs          && \
    echo ""             > services/pnl-grpc/src/lib.rs                && \
    echo "fn main() {}" > services/pnl-grpc/src/main.rs              && \
    echo "fn main() {}" > services/pnl-grpc/src/bin/client.rs

# Touch real sources to force recompile of application code
RUN find shared/observability/src services/ledger-service/src -name "*.rs" | xargs touch && \
    cargo build --release --package ledger-service

# ─── Stage 2: Runtime ────────────────────────────────────────────────────────
FROM debian:trixie-slim AS runtime

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN groupadd --gid 1001 ledger && \
    useradd --uid 1001 --gid ledger --no-create-home ledger

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/ledger-service ./ledger-service

# Own the binary
RUN chown ledger:ledger ./ledger-service

USER ledger

EXPOSE 3000

ENTRYPOINT ["./ledger-service"]
