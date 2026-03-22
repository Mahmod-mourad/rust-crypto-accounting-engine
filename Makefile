.PHONY: build build-release run run-consumer test lint fmt fmt-check \
        docker-build docker-build-consumer docker-run \
        up down logs clean

# ─── Local Development ───────────────────────────────────────────────────────

## Build the workspace in debug mode
build:
	cargo build

## Build the workspace in release mode
build-release:
	cargo build --release

## Run the ledger service (loads .env automatically via dotenvy)
run:
	cd services/ledger-service && cargo run

## Run the pnl-consumer (loads .env automatically via dotenvy)
run-consumer:
	cd services/pnl-consumer && cargo run

## Run all tests in the workspace
test:
	cargo test

## Lint with clippy (fail on warnings)
lint:
	cargo clippy -- -D warnings

## Format all source files
fmt:
	cargo fmt --all

## Check formatting without modifying files
fmt-check:
	cargo fmt --all -- --check

# ─── Docker (individual images) ──────────────────────────────────────────────

LEDGER_IMAGE   ?= ledger-service
CONSUMER_IMAGE ?= pnl-consumer
IMAGE_TAG      ?= latest

## Build the ledger-service Docker image
docker-build:
	docker build -t $(LEDGER_IMAGE):$(IMAGE_TAG) -f Dockerfile .

## Build the pnl-consumer Docker image
docker-build-consumer:
	docker build -t $(CONSUMER_IMAGE):$(IMAGE_TAG) -f Dockerfile.pnl-consumer .

## Run the ledger-service container (requires external Postgres + Kafka)
docker-run:
	docker run --rm \
		--env-file services/ledger-service/.env \
		-p 3000:3000 \
		$(LEDGER_IMAGE):$(IMAGE_TAG)

# ─── Docker Compose (full stack) ─────────────────────────────────────────────

## Start the full stack (Kafka + PostgreSQL + both services) in the background
up:
	docker compose up --build -d

## Stream logs from all containers
logs:
	docker compose logs -f

## Stop and remove all containers (data volumes are preserved)
down:
	docker compose down

## Stop and remove containers AND volumes (wipes all data)
down-volumes:
	docker compose down -v

# ─── Utilities ───────────────────────────────────────────────────────────────

## Remove build artifacts
clean:
	cargo clean
