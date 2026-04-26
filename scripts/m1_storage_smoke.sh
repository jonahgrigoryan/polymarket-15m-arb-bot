#!/usr/bin/env bash
set -euo pipefail

CLICKHOUSE_URL="${CLICKHOUSE_URL:-http://localhost:8123}"
POSTGRES_URL="${POSTGRES_URL:-postgres://polymarket:polymarket@localhost:5432/polymarket_15m}"
CLICKHOUSE_IMAGE="${CLICKHOUSE_IMAGE:-clickhouse/clickhouse-server:24.12-alpine}"
POSTGRES_IMAGE="${POSTGRES_IMAGE:-postgres:16-alpine}"

POSTGRES_CONTAINER=""
CLICKHOUSE_CONTAINER=""

cleanup() {
  if [[ -n "${POSTGRES_CONTAINER}" ]]; then
    docker rm -f "${POSTGRES_CONTAINER}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${CLICKHOUSE_CONTAINER}" ]]; then
    docker rm -f "${CLICKHOUSE_CONTAINER}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "m1_storage_smoke=starting"
echo "clickhouse_url=${CLICKHOUSE_URL}"
echo "postgres_url=${POSTGRES_URL}"

if ! command -v curl >/dev/null 2>&1; then
  echo "missing_required_command=curl" >&2
  exit 1
fi

if ! command -v psql >/dev/null 2>&1 && ! command -v docker >/dev/null 2>&1; then
  echo "missing_required_command=psql_or_docker" >&2
  exit 1
fi

wait_for() {
  local label="$1"
  local attempts="$2"
  shift 2

  for _ in $(seq 1 "${attempts}"); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  echo "${label}=not_ready" >&2
  return 1
}

clickhouse_ping() {
  curl -fsS "${CLICKHOUSE_URL}/ping"
}

start_clickhouse_container() {
  CLICKHOUSE_CONTAINER="p15m-m1-clickhouse-$$"
  echo "clickhouse=starting_docker image=${CLICKHOUSE_IMAGE}"
  docker run -d \
    --name "${CLICKHOUSE_CONTAINER}" \
    -e CLICKHOUSE_SKIP_USER_SETUP=1 \
    -p 127.0.0.1::8123 \
    "${CLICKHOUSE_IMAGE}" >/dev/null

  local port
  port="$(docker port "${CLICKHOUSE_CONTAINER}" 8123/tcp | awk -F: '{print $NF}')"
  CLICKHOUSE_URL="http://127.0.0.1:${port}"
  echo "clickhouse_url=${CLICKHOUSE_URL}"
  wait_for "clickhouse" 60 clickhouse_ping
}

start_postgres_container() {
  POSTGRES_CONTAINER="p15m-m1-postgres-$$"
  echo "postgres=starting_docker image=${POSTGRES_IMAGE}"
  docker run -d \
    --name "${POSTGRES_CONTAINER}" \
    -e POSTGRES_USER=polymarket \
    -e POSTGRES_PASSWORD=polymarket \
    -e POSTGRES_DB=polymarket_15m \
    -p 127.0.0.1::5432 \
    "${POSTGRES_IMAGE}" >/dev/null

  local port
  port="$(docker port "${POSTGRES_CONTAINER}" 5432/tcp | awk -F: '{print $NF}')"
  POSTGRES_URL="postgres://polymarket:polymarket@127.0.0.1:${port}/polymarket_15m"
  echo "postgres_url=${POSTGRES_URL}"
  wait_for "postgres" 60 docker exec "${POSTGRES_CONTAINER}" \
    pg_isready -U polymarket -d polymarket_15m
}

psql_file() {
  local file="$1"
  if [[ -n "${POSTGRES_CONTAINER}" ]]; then
    docker exec -i "${POSTGRES_CONTAINER}" \
      psql -U polymarket -d polymarket_15m -v ON_ERROR_STOP=1 <"${file}"
  else
    psql "${POSTGRES_URL}" -v ON_ERROR_STOP=1 -f "${file}"
  fi
}

psql_stdin() {
  if [[ -n "${POSTGRES_CONTAINER}" ]]; then
    docker exec -i "${POSTGRES_CONTAINER}" \
      psql -U polymarket -d polymarket_15m -v ON_ERROR_STOP=1
  else
    psql "${POSTGRES_URL}" -v ON_ERROR_STOP=1
  fi
}

psql_scalar() {
  local query="$1"
  if [[ -n "${POSTGRES_CONTAINER}" ]]; then
    docker exec -i "${POSTGRES_CONTAINER}" \
      psql -U polymarket -d polymarket_15m -tA -v ON_ERROR_STOP=1 -c "${query}"
  else
    psql "${POSTGRES_URL}" -tA -v ON_ERROR_STOP=1 -c "${query}"
  fi
}

clickhouse_file() {
  local file="$1"
  local statement=""

  while IFS= read -r line || [[ -n "${line}" ]]; do
    statement+="${line}"$'\n'

    if [[ "${line}" == *";" ]]; then
      printf "%s" "${statement}" | curl -fsS "${CLICKHOUSE_URL}/" --data-binary @- >/dev/null
      statement=""
    fi
  done <"${file}"

  if [[ -n "${statement//[[:space:]]/}" ]]; then
    printf "%s" "${statement}" | curl -fsS "${CLICKHOUSE_URL}/" --data-binary @- >/dev/null
  fi
}

if ! clickhouse_ping >/dev/null 2>&1; then
  if command -v docker >/dev/null 2>&1; then
    start_clickhouse_container
  else
    echo "clickhouse=unreachable url=${CLICKHOUSE_URL}" >&2
    exit 1
  fi
fi

if ! command -v psql >/dev/null 2>&1; then
  start_postgres_container
fi

echo "clickhouse=ping"
curl -fsS "${CLICKHOUSE_URL}/ping" >/dev/null

echo "clickhouse=apply_migration"
clickhouse_file migrations/clickhouse/0001_events.sql

echo "clickhouse=sample_event_write"
curl -fsS "${CLICKHOUSE_URL}/" \
  --data-binary "INSERT INTO normalized_events FORMAT JSONEachRow
{\"run_id\":\"m1-smoke\",\"event_id\":\"event-1\",\"event_type\":\"replay_checkpoint\",\"source\":\"m1-smoke\",\"source_ts\":null,\"recv_wall_ts\":1777000000000,\"recv_mono_ns\":1,\"ingest_seq\":1,\"market_id\":null,\"asset\":null,\"payload\":\"{\\\"type\\\":\\\"replay_checkpoint\\\",\\\"data\\\":{\\\"replay_run_id\\\":\\\"m1-smoke\\\",\\\"event_count\\\":1,\\\"checkpoint_ts\\\":1777000000000}}\"}" >/dev/null

clickhouse_count="$(curl -fsS "${CLICKHOUSE_URL}/" \
  --data-binary "SELECT count() FROM normalized_events WHERE run_id = 'm1-smoke' AND event_id = 'event-1'")"

if [[ "${clickhouse_count}" != "1" ]]; then
  echo "clickhouse_sample_event_read=failed count=${clickhouse_count}" >&2
  exit 1
fi

echo "postgres=apply_migration"
psql_file migrations/postgres/0001_relational_state.sql >/dev/null

echo "postgres=sample_market_and_config_write"
psql_stdin >/dev/null <<'SQL'
INSERT INTO markets (
  market_id,
  slug,
  title,
  asset,
  condition_id,
  start_ts,
  end_ts,
  resolution_source,
  tick_size,
  min_order_size,
  lifecycle_state,
  payload
) VALUES (
  'm1-market',
  'm1-market',
  'M1 Smoke Market',
  'BTC',
  'm1-condition',
  1777000000000,
  1777000900000,
  'm1-resolution-source',
  0.01,
  5.0,
  'active',
  '{"market_id":"m1-market"}'::jsonb
) ON CONFLICT (market_id) DO UPDATE SET updated_at = now();

INSERT INTO config_snapshots (
  run_id,
  captured_wall_ts,
  config
) VALUES (
  'm1-smoke',
  1777000000000,
  '{"runtime":{"mode":"validate"}}'::jsonb
) ON CONFLICT (run_id) DO UPDATE SET config = excluded.config;
SQL

postgres_count="$(psql_scalar "SELECT count(*) FROM markets WHERE market_id = 'm1-market'")"

if [[ "${postgres_count}" != "1" ]]; then
  echo "postgres_sample_market_read=failed count=${postgres_count}" >&2
  exit 1
fi

postgres_config_count="$(psql_scalar "SELECT count(*) FROM config_snapshots WHERE run_id = 'm1-smoke'")"

if [[ "${postgres_config_count}" != "1" ]]; then
  echo "postgres_sample_config_read=failed count=${postgres_config_count}" >&2
  exit 1
fi

echo "m1_storage_smoke=ok"
