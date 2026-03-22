# Rust Crypto Accounting Engine

A production-grade, event-driven cryptocurrency accounting system built in Rust. Tracks trades across multiple assets, computes FIFO cost-basis P&L in real time, and exposes results over both REST and gRPC — with full observability baked in.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Data Flow](#data-flow)
- [Key Design Decisions](#key-design-decisions)
- [How to Run](#how-to-run)
- [API Reference](#api-reference)
- [Observability](#observability)
- [Kubernetes Deployment](#kubernetes-deployment)
- [Scaling Discussion](#scaling-discussion)

---

## Overview

This system solves a core problem in crypto portfolio management: **accurately computing realized and unrealized P&L across thousands of trades, without floating-point errors, at scale.**

It handles the full lifecycle:

1. **Ingest** — trades arrive via REST (`POST /trades`)
2. **Stream** — trade events are published to Kafka
3. **Process** — a stateful consumer applies FIFO lot accounting and persists P&L snapshots
4. **Query** — a gRPC server exposes portfolio state to downstream clients

Every component is written in safe, async Rust and designed to run in Kubernetes.

---

## Architecture

```
                         ┌─────────────────────────────────────────────────────────────┐
                         │                      Client Applications                     │
                         └──────────────────┬──────────────────────────┬───────────────┘
                                            │ REST                     │ gRPC
                                            ▼                          ▼
                         ┌──────────────────────────┐    ┌─────────────────────────────┐
                         │      ledger-service       │    │         pnl-grpc            │
                         │  ┌────────────────────┐   │    │  ┌───────────────────────┐  │
                         │  │   API Layer (Axum) │   │    │  │ gRPC Service (Tonic)  │  │
                         │  ├────────────────────┤   │    │  ├───────────────────────┤  │
                         │  │  Application Layer  │   │    │  │   Read-only queries   │  │
                         │  ├────────────────────┤   │    │  └───────────────────────┘  │
                         │  │   Domain Layer      │   │    │          │ :50051           │
                         │  │  (FIFO, Portfolio)  │   │    └──────────┼──────────────────┘
                         │  ├────────────────────┤   │               │
                         │  │  Infrastructure     │   │    ┌──────────▼──────────────────┐
                         │  │  (DB + Kafka)       │   │    │          pnl_db             │
                         │  └─────────┬───────────┘   │    │  ┌─────────────────────┐   │
                         └───────────┬┴───────────────┘    │  │  portfolio_state    │   │
                                     │                      │  │  pnl_snapshots      │   │
                              ┌──────┴──────┐               │  │  processed_events   │   │
                              │  ledger_db  │               │  └─────────────────────┘   │
                              │  (trades)   │               └──────────▲──────────────────┘
                              └─────────────┘                          │
                                     │                                 │
                                     │ Kafka Topic: trades             │ PostgreSQL
                                     ▼                                 │ transactions
                         ┌───────────────────────────────────────────────────────────────┐
                         │                       Apache Kafka                             │
                         │  ┌──────────────────────────────────────────────────────────┐ │
                         │  │              Topic: trades (partitioned)                  │ │
                         │  └──────────────────────────────────────────────────────────┘ │
                         └───────────────────────────────────────────────────────────────┘
                                                      │
                                                      ▼
                         ┌─────────────────────────────────────────────────────────────┐
                         │                       pnl-consumer                           │
                         │  ┌───────────────────────────────────────────────────────┐  │
                         │  │  KafkaConsumer (rdkafka, manual offset commit)         │  │
                         │  ├───────────────────────────────────────────────────────┤  │
                         │  │  EventProcessor                                        │  │
                         │  │  1. claim_event() — idempotency via INSERT ON CONFLICT │  │
                         │  │  2. load_portfolio() — SELECT ... FOR UPDATE           │  │
                         │  │  3. portfolio.apply(event) — FIFO lot accounting       │  │
                         │  │  4. save_portfolio() + save_pnl_snapshot()             │  │
                         │  │  5. Commit Kafka offset                                │  │
                         │  └───────────────────────────────────────────────────────┘  │
                         └─────────────────────────────────────────────────────────────┘

                         ┌─────────────────────────────────────────────────────────────┐
                         │                      Observability                           │
                         │   Jaeger (traces)    Prometheus (metrics)    Grafana (UI)    │
                         │      :16686               :9090                  :3001        │
                         └─────────────────────────────────────────────────────────────┘
```

### Service Responsibilities

| Service | Role | Protocol |
|---|---|---|
| `ledger-service` | Trade ingestion, in-memory portfolio, Kafka publishing | REST :3000 |
| `pnl-consumer` | Stateful Kafka consumer, FIFO P&L computation, DB persistence | — |
| `pnl-grpc` | Read-only query server over portfolio state | gRPC :50051 |
| `shared/observability` | Common telemetry bootstrap (logs, traces, metrics) | — |

---

## Tech Stack

| Layer | Technology | Rationale |
|---|---|---|
| Language | **Rust** | Memory safety, zero-cost abstractions, no GC pauses in critical paths |
| Async Runtime | **Tokio** | Production async executor; battle-tested for network services |
| REST API | **Axum 0.7** | Tower-native, composable middleware, type-safe extractors |
| gRPC | **Tonic + Prost** | First-class Rust gRPC; proto3 with compile-time codegen |
| Database | **PostgreSQL 16** | ACID transactions, row-level locking, `NUMERIC(28,10)` for precision |
| ORM / Query | **SQLx 0.8** | Compile-time SQL verification; async-native; no ORM overhead |
| Message Broker | **Apache Kafka (KRaft)** | Durable, ordered, replayable event log; no ZooKeeper |
| Kafka Client | **rdkafka 0.36** | librdkafka bindings; manual offset commit for at-least-once |
| Decimal Math | **rust_decimal** | Exact decimal arithmetic — no floats in any financial path |
| Logging | **tracing + tracing-subscriber** | Structured JSON logs; async-aware span context |
| Distributed Tracing | **OpenTelemetry + Jaeger** | Cross-service trace propagation via OTLP/gRPC |
| Metrics | **metrics + Prometheus exporter** | Counters, gauges, histograms at `:9091/metrics` |
| Containerization | **Docker (multi-stage)** | Minimal runtime images from `debian:trixie-slim` |
| Orchestration | **Kubernetes** | Namespace isolation, StatefulSet for Postgres, Secrets for creds |
| Local Dev | **Docker Compose** | Full 8-service stack with a single `make up` |
| Benchmarks | **Criterion 0.5** | Statistical regression benchmarks for core portfolio operations |
| Error Handling | **thiserror + anyhow** | Typed domain errors; contextual application errors |

---

## Project Structure

```
rust-crypto-accounting-engine/
├── Cargo.toml                          # Workspace (resolver = "2")
├── Cargo.lock
│
├── services/
│   ├── ledger-service/                 # REST API — trade ingestion
│   │   ├── src/
│   │   │   ├── api/                    # Axum handlers, router, state, extractors
│   │   │   ├── application/            # Use cases, DTOs
│   │   │   ├── domain/                 # Portfolio, Trade, PnL (pure logic, no I/O)
│   │   │   ├── infrastructure/         # PostgreSQL repo, Kafka producer
│   │   │   └── config/                 # Environment-based config
│   │   ├── migrations/
│   │   ├── benches/portfolio.rs        # Criterion benchmarks
│   │   └── .env.example
│   │
│   ├── pnl-consumer/                   # Kafka consumer — stateful P&L processor
│   │   ├── src/
│   │   │   ├── application/processor.rs # Idempotent event processing
│   │   │   ├── domain/                  # TradeEvent, Portfolio state machine
│   │   │   └── infrastructure/          # KafkaConsumer, PnlRepository
│   │   ├── migrations/
│   │   └── .env.example
│   │
│   └── pnl-grpc/                       # gRPC server — read-only P&L queries
│       ├── src/
│       │   ├── service.rs              # PnlServiceImpl
│       │   └── bin/client.rs           # Demo client
│       ├── proto/pnl.proto
│       └── build.rs                    # tonic-build proto compilation
│
├── shared/
│   └── observability/                  # Shared telemetry bootstrap
│       └── src/lib.rs                  # init(), TelemetryGuard, metric macros
│
├── k8s/                                # Kubernetes manifests (namespace → services)
├── prometheus/prometheus.yml
├── Dockerfile                          # ledger-service (multi-stage)
├── Dockerfile.pnl-consumer
├── Dockerfile.pnl-grpc
├── docker-compose.yml                  # Full local stack
└── Makefile
```

---

## Data Flow

### Trade Ingestion → P&L Computation

```
POST /trades
  {
    "asset": "BTC",
    "side": "buy",
    "quantity": "1.5",
    "price": "42000.00"
  }
        │
        ▼
  Validate + persist to ledger_db.trades
        │
        ├──► Return HTTP 201 (immediate)
        │
        └──► Publish TradeEvent to Kafka topic: trades
                        │
                        ▼
               pnl-consumer receives event
                        │
                        ├─ 1. INSERT INTO processed_events (idempotency guard)
                        │      ON CONFLICT → skip (duplicate)
                        │
                        ├─ 2. SELECT * FROM portfolio_state WHERE asset = 'BTC'
                        │      FOR UPDATE  ← serialises concurrent consumers
                        │
                        ├─ 3. Apply FIFO lot logic
                        │      Buy  → append Lot to VecDeque
                        │      Sell → drain lots front-to-back, emit RealizedPnL per lot
                        │
                        ├─ 4. UPDATE portfolio_state (new lot queue, running PnL)
                        │
                        ├─ 5. INSERT INTO pnl_snapshots (immutable audit record)
                        │
                        └─ 6. Commit Kafka offset
```

### FIFO Lot Accounting Example

```
Trade history for BTC:
  BUY   10 BTC @ $100   →  Lot A: 10 × $100
  BUY    5 BTC @ $200   →  Lot B:  5 × $200
  SELL  12 BTC @ $300

FIFO sell resolution:
  Consume Lot A (10 BTC @ $100):  gain = 10 × ($300 − $100) = $2,000
  Consume Lot B (partial, 2 BTC): gain =  2 × ($300 − $200) =   $200
  ──────────────────────────────────────────────────────────────────
  Realized P&L this trade:                                  = $2,200
  Remaining open lots:            3 BTC @ $200
```

---

## Key Design Decisions

### Exactly-Once Semantics over Kafka's At-Least-Once

Kafka guarantees at-least-once delivery. Rather than relying on Kafka transactions (which require co-located producers and consumers), the consumer achieves idempotency at the database layer:

```sql
INSERT INTO processed_events (event_id, ...) VALUES ($1, ...)
ON CONFLICT (event_id) DO NOTHING
RETURNING event_id
```

If the query returns no rows, the event was already processed — the handler returns early. Combined with a wrapping PostgreSQL transaction, this gives effectively-once semantics.

### Row-Level Locking for Per-Asset Consistency

Multiple `pnl-consumer` instances may process events for the same asset concurrently (Kafka does not partition by asset). The `FOR UPDATE` lock on `portfolio_state` serialises writes per asset, preventing torn portfolio state without requiring a single-threaded consumer.

### No Floating-Point in the Financial Path

All prices, quantities, and P&L values use `rust_decimal::Decimal`. PostgreSQL columns are typed `NUMERIC(28, 10)`. This eliminates the class of bugs caused by IEEE 754 rounding (e.g., `0.1 + 0.2 ≠ 0.3`).

### Layered Architecture per Service

Each service follows a strict dependency rule:

```
Domain  ←  Application  ←  Infrastructure  ←  API / Entrypoint
```

The domain layer has zero external dependencies — it compiles and tests without a database or Kafka. This makes domain logic fast to test and easy to reason about.

### Non-Fatal External Dependencies in ledger-service

`ledger-service` starts successfully even if PostgreSQL or Kafka are unavailable at boot. P&L can be computed in-memory against the in-process portfolio model. This improves resilience during rolling deploys and infrastructure hiccups.

---

## How to Run

### Prerequisites

- Docker + Docker Compose
- Rust toolchain (`rustup` — for local development)
- `protoc` (Protocol Buffers compiler — for `pnl-grpc` local builds)

### Full Stack (Recommended)

```bash
# Start all 8 services: Postgres, Kafka, Jaeger, Prometheus, Grafana,
# ledger-service, pnl-consumer, pnl-grpc
make up

# Tail logs
make logs

# Tear down (keep volumes)
make down

# Tear down + wipe all data
make down-volumes
```

Service endpoints once running:

| Service | URL |
|---|---|
| ledger-service REST | http://localhost:3000 |
| pnl-grpc | localhost:50051 |
| Jaeger UI | http://localhost:16686 |
| Prometheus | http://localhost:9090 |
| Grafana | http://localhost:3001 |
| PostgreSQL | localhost:5433 |
| Kafka | localhost:9094 |

### Local Development (without Docker)

```bash
# 1. Start infrastructure only
docker compose up postgres kafka jaeger -d

# 2. Configure environment
cp services/ledger-service/.env.example services/ledger-service/.env
cp services/pnl-consumer/.env.example   services/pnl-consumer/.env

# 3. Run services in separate terminals
make run           # ledger-service  → :3000
make run-consumer  # pnl-consumer
cargo run -p pnl-grpc  # pnl-grpc → :50051
```

### Build & Test

```bash
make build          # Debug build
make build-release  # Optimised release build
make test           # Full test suite
make lint           # cargo clippy -D warnings
make fmt            # cargo fmt --all
```

### Quick Smoke Test

```bash
# Create a buy trade
curl -s -X POST http://localhost:3000/trades \
  -H 'Content-Type: application/json' \
  -d '{"asset":"BTC","side":"buy","quantity":"2.0","price":"45000.00"}' | jq

# Create a sell trade
curl -s -X POST http://localhost:3000/trades \
  -H 'Content-Type: application/json' \
  -d '{"asset":"BTC","side":"sell","quantity":"1.0","price":"50000.00"}' | jq

# Query P&L
curl -s http://localhost:3000/pnl | jq

# Query portfolio
curl -s http://localhost:3000/portfolio | jq

# Query via gRPC (demo client)
cargo run -p pnl-grpc --bin client
```

---

## API Reference

### ledger-service (REST :3000)

| Method | Path | Description |
|---|---|---|
| `POST` | `/trades` | Record a new trade |
| `GET` | `/trades` | List all trades |
| `GET` | `/portfolio` | Current portfolio positions |
| `GET` | `/pnl` | Realized + unrealized P&L |
| `GET` | `/health` | Health check |

**POST /trades — Request**
```json
{
  "asset":    "BTC",
  "side":     "buy",
  "quantity": "1.5",
  "price":    "42000.00"
}
```

**GET /pnl — Response**
```json
{
  "realized_pnl": "5000.00",
  "unrealized_pnl": "1250.00",
  "positions": [
    {
      "asset": "BTC",
      "quantity": "0.5",
      "average_cost": "42000.00",
      "current_value": "43250.00"
    }
  ]
}
```

### pnl-grpc (gRPC :50051)

```protobuf
service PnlService {
  rpc GetPnlSummary(GetPnlSummaryRequest) returns (GetPnlSummaryResponse);
  rpc ListAssets(ListAssetsRequest)       returns (ListAssetsResponse);
}
```

`GetPnlSummaryResponse` includes: `asset`, `total_realized_pnl`, `total_quantity`, and the full list of open `Lot` records (quantity + cost_per_unit per lot).

---

## Observability

All three services share the `observability` crate and emit three telemetry signals:

### Structured Logs

JSON-formatted, shipped to stdout. Consumed by any log aggregation pipeline (Loki, Datadog, CloudWatch).

```json
{"timestamp":"2024-01-15T10:23:01Z","level":"INFO","service":"ledger-service",
 "message":"trade created","trade_id":"uuid","asset":"BTC","side":"buy","quantity":"1.5"}
```

### Distributed Traces

OpenTelemetry spans exported via gRPC to Jaeger. Trace context propagates across HTTP and Kafka boundaries, giving end-to-end visibility from `POST /trades` through Kafka processing to the P&L snapshot insert.

**Jaeger UI:** http://localhost:16686

### Prometheus Metrics

Each service exposes `/metrics` on port `9091`. Key metrics:

| Metric | Type | Labels |
|---|---|---|
| `pnl_consumer_events_total` | Counter | `status=processed\|skipped` |
| `pnl_consumer_processing_duration_seconds` | Histogram | `asset` |
| `pnl_grpc_requests_total` | Counter | `method`, `status` |

**Prometheus:** http://localhost:9090
**Grafana:** http://localhost:3001

---

## Kubernetes Deployment

Manifests in `k8s/` deploy the full stack to any Kubernetes cluster:

```bash
kubectl apply -f k8s/
```

Manifest order:

```
00-namespace.yaml          # Isolated namespace
01-secrets.yaml            # DB credentials (base64 encoded)
02-postgres-init-configmap.yaml  # Init SQL (creates ledger_db + pnl_db)
03-postgres.yaml           # StatefulSet + PersistentVolumeClaim
04-kafka.yaml              # Deployment (KRaft, no ZooKeeper)
05-ledger-service.yaml     # Deployment + ClusterIP Service
06-pnl-consumer.yaml       # Deployment
07-pnl-grpc.yaml           # Deployment + ClusterIP Service
```

Production hardening checklist for this manifests:
- Replace the in-cluster Postgres StatefulSet with a managed RDS/Cloud SQL instance
- Replace Secrets with an external secrets operator (e.g., External Secrets + AWS Secrets Manager)
- Add `PodDisruptionBudget` for ledger-service and pnl-grpc
- Add `HorizontalPodAutoscaler` on CPU/memory for ledger-service and pnl-grpc

---

## Scaling Discussion

### Horizontal Scaling — ledger-service

`ledger-service` is stateless beyond its database connection pool. It can be scaled horizontally behind a load balancer without any coordination. The bottleneck at scale is the PostgreSQL write path (trade inserts) and Kafka producer throughput, both of which are high-throughput by design.

### Horizontal Scaling — pnl-consumer

The consumer scales by adding more instances to the same Kafka consumer group. Kafka distributes partitions across instances. The current bottleneck is **per-asset write contention** in PostgreSQL: two consumers processing the same asset concurrently will serialize on the `FOR UPDATE` lock.

Mitigation strategies:

1. **Partition by asset key** — Configure the Kafka producer to partition by `asset`. All events for a given asset land on the same partition, so only one consumer instance ever processes that asset. This eliminates lock contention and allows parallelism proportional to the number of distinct assets.

2. **In-memory state + periodic flush** — Accumulate portfolio state in memory and flush to PostgreSQL in batches. Trades for the same asset within a batch window are coalesced before any DB write, dramatically reducing write amplification. Requires careful crash recovery design (replay from last committed Kafka offset).

3. **Sharded consumers** — Route assets to dedicated consumer instances via consistent hashing. Each instance owns a disjoint asset shard, eliminating cross-instance contention entirely.

### Horizontal Scaling — pnl-grpc

`pnl-grpc` is read-only. It can scale horizontally without any coordination. At high read throughput, add a PostgreSQL read replica and point pnl-grpc instances at it.

### Database Scaling

- **Read replicas** — pnl-grpc queries are read-only and can be served from replicas
- **Table partitioning** — `pnl_snapshots` grows unboundedly; range partition by `trade_ts` for efficient archival and query planning
- **Connection pooling** — PgBouncer in front of PostgreSQL to cap connection count under high concurrency

### Kafka Scaling

- Increase topic partition count to match consumer parallelism
- Enable Kafka log compaction on `portfolio_state` if a compacted topic pattern is adopted
- Use Kafka consumer group lag as an autoscaling metric for pnl-consumer HPA

### Financial Precision at Scale

`NUMERIC(28, 10)` supports 28 significant digits — sufficient for any realistic crypto position or P&L value. The `rust_decimal` crate uses 128-bit integer arithmetic internally, ensuring no precision loss regardless of throughput.

---

## License

MIT
