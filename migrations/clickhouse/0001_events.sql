CREATE TABLE IF NOT EXISTS raw_messages
(
    run_id String,
    source LowCardinality(String),
    recv_wall_ts Int64,
    recv_mono_ns UInt64,
    ingest_seq UInt64,
    payload String,
    inserted_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (run_id, recv_mono_ns, ingest_seq, source);

CREATE TABLE IF NOT EXISTS normalized_events
(
    run_id String,
    event_id String,
    event_type LowCardinality(String),
    source LowCardinality(String),
    source_ts Nullable(Int64),
    recv_wall_ts Int64,
    recv_mono_ns UInt64,
    ingest_seq UInt64,
    market_id Nullable(String),
    asset Nullable(String),
    payload String,
    inserted_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (run_id, recv_mono_ns, ingest_seq, event_id);

CREATE TABLE IF NOT EXISTS replay_checkpoints
(
    run_id String,
    replay_run_id String,
    event_count UInt64,
    checkpoint_ts Int64,
    payload String,
    inserted_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (run_id, replay_run_id, event_count);
