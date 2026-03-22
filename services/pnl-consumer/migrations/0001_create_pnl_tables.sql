-- ─── Idempotency ─────────────────────────────────────────────────────────────
--
-- Every TradeEvent carries a unique `event_id`.  Before processing we INSERT
-- into this table; ON CONFLICT means the event was already handled — skip it.

CREATE TABLE processed_events (
    event_id        UUID        PRIMARY KEY,
    trade_id        UUID        NOT NULL,
    asset           TEXT        NOT NULL,
    kafka_partition INT         NOT NULL,
    kafka_offset    BIGINT      NOT NULL,
    processed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_processed_events_trade_id ON processed_events (trade_id);
CREATE INDEX idx_processed_events_asset    ON processed_events (asset);

-- ─── Portfolio state ──────────────────────────────────────────────────────────
--
-- One row per asset.  `lots` is a JSONB array of { quantity, cost_per_unit }
-- objects representing the FIFO queue.  The row is updated atomically with
-- the idempotency insert so state is always consistent with what was processed.

CREATE TABLE portfolio_state (
    asset               TEXT        PRIMARY KEY,
    -- FIFO queue of open lots: [{"quantity":"1.5","cost_per_unit":"45000"}]
    lots                JSONB       NOT NULL DEFAULT '[]',
    total_quantity      NUMERIC(28, 10) NOT NULL DEFAULT 0,
    total_realized_pnl  NUMERIC(28, 10) NOT NULL DEFAULT 0,
    last_event_id       UUID,
    last_trade_ts       TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── PnL snapshots ───────────────────────────────────────────────────────────
--
-- Immutable record of what PnL was computed for each processed event.
-- Useful for auditing, replaying, or feeding downstream consumers.

CREATE TABLE pnl_snapshots (
    id                  UUID        PRIMARY KEY,
    event_id            UUID        NOT NULL REFERENCES processed_events(event_id),
    trade_id            UUID        NOT NULL,
    asset               TEXT        NOT NULL,
    trade_side          TEXT        NOT NULL,
    trade_quantity      NUMERIC(28, 10) NOT NULL,
    trade_price         NUMERIC(28, 10) NOT NULL,
    -- PnL realized by this specific trade (sum across FIFO lots; 0 for buys)
    realized_pnl        NUMERIC(28, 10) NOT NULL,
    -- Running total for this asset after applying this trade
    total_realized_pnl  NUMERIC(28, 10) NOT NULL,
    trade_ts            TIMESTAMPTZ NOT NULL,
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pnl_snapshots_asset    ON pnl_snapshots (asset);
CREATE INDEX idx_pnl_snapshots_trade_ts ON pnl_snapshots (trade_ts DESC);
CREATE INDEX idx_pnl_snapshots_event_id ON pnl_snapshots (event_id);
