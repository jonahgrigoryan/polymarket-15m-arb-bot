CREATE TABLE IF NOT EXISTS markets
(
    market_id TEXT PRIMARY KEY,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    asset TEXT NOT NULL,
    condition_id TEXT NOT NULL,
    start_ts BIGINT NOT NULL,
    end_ts BIGINT NOT NULL,
    resolution_source TEXT,
    tick_size DOUBLE PRECISION NOT NULL,
    min_order_size DOUBLE PRECISION NOT NULL,
    lifecycle_state TEXT NOT NULL,
    ineligibility_reason TEXT,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS config_snapshots
(
    run_id TEXT PRIMARY KEY,
    captured_wall_ts BIGINT NOT NULL,
    config JSONB NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS paper_orders
(
    order_id TEXT PRIMARY KEY,
    market_id TEXT NOT NULL,
    token_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    side TEXT NOT NULL,
    order_kind TEXT NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    filled_size DOUBLE PRECISION NOT NULL,
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS paper_fills
(
    fill_id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    market_id TEXT NOT NULL,
    token_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    side TEXT NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    fee_paid DOUBLE PRECISION NOT NULL,
    liquidity TEXT NOT NULL,
    filled_ts BIGINT NOT NULL,
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS paper_positions
(
    market_id TEXT NOT NULL,
    token_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    average_price DOUBLE PRECISION NOT NULL,
    realized_pnl DOUBLE PRECISION NOT NULL,
    unrealized_pnl DOUBLE PRECISION NOT NULL,
    updated_ts BIGINT NOT NULL,
    payload JSONB NOT NULL,
    PRIMARY KEY (market_id, token_id)
);

CREATE TABLE IF NOT EXISTS paper_balances
(
    run_id TEXT PRIMARY KEY,
    starting_balance DOUBLE PRECISION NOT NULL,
    cash_balance DOUBLE PRECISION NOT NULL,
    realized_pnl DOUBLE PRECISION NOT NULL,
    unrealized_pnl DOUBLE PRECISION NOT NULL,
    updated_ts BIGINT NOT NULL,
    payload JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS risk_events
(
    run_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    halted BOOLEAN NOT NULL,
    reason TEXT,
    updated_ts BIGINT NOT NULL,
    payload JSONB NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, event_id)
);

CREATE TABLE IF NOT EXISTS replay_runs
(
    replay_run_id TEXT PRIMARY KEY,
    source_run_id TEXT NOT NULL,
    started_wall_ts BIGINT NOT NULL,
    completed_wall_ts BIGINT,
    deterministic BOOLEAN NOT NULL,
    result TEXT,
    report_path TEXT,
    payload JSONB NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
