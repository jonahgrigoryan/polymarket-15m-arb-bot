#!/usr/bin/env bash
set -euo pipefail

echo "== rust formatting =="
cargo fmt --check

echo "== rust tests =="
cargo test --offline

echo "== rust clippy =="
cargo clippy --offline -- -D warnings

echo "== diff whitespace =="
git diff --check

echo "== safety scope scan =="
set +e
rg -n -i \
  --glob '!config/local.toml' \
  --glob '!.env' \
  "(cancel.?all|batch|FOK|FAK|marketable|taker)" \
  src config runbooks verification
safety_scan_status=$?
set -e
if [ "$safety_scan_status" -eq 1 ]; then
  echo "No safety scope scan hits."
elif [ "$safety_scan_status" -ne 0 ]; then
  exit "$safety_scan_status"
fi

echo "== no-secret scan =="
set +e
rg -n -i \
  --glob '!config/local.toml' \
  --glob '!.env' \
  "(wallet|private.*key|secret|passphrase|mnemonic|seed|0x[0-9a-fA-F]{64})" \
  src config runbooks verification
secret_scan_status=$?
set -e
if [ "$secret_scan_status" -eq 1 ]; then
  echo "No no-secret scan hits."
elif [ "$secret_scan_status" -ne 0 ]; then
  exit "$secret_scan_status"
fi

echo "== ignored local secret files =="
test ! -e .env || git check-ignore .env >/dev/null
test ! -e config/local.toml || git check-ignore config/local.toml >/dev/null
