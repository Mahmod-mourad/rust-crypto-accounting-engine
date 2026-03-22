-- Create the trades table for persistent storage of all executed trades.
--
-- quantity and price use NUMERIC(28, 10) to preserve full Decimal precision
-- without floating-point rounding errors.
-- side is constrained to 'buy' or 'sell' at the DB level to prevent bad data.

CREATE TABLE IF NOT EXISTS trades (
    id         UUID             PRIMARY KEY,
    asset      TEXT             NOT NULL,
    quantity   NUMERIC(28, 10)  NOT NULL,
    price      NUMERIC(28, 10)  NOT NULL,
    side       TEXT             NOT NULL CHECK (side IN ('buy', 'sell')),
    timestamp  TIMESTAMPTZ      NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trades_asset     ON trades (asset);
CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades (timestamp DESC);
