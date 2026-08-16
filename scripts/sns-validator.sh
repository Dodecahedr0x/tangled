#!/usr/bin/env bash
# Start solana-test-validator with SNS name-account fixtures (same as CI).
#
# CI installs Agave v4.2.1 (`solana-test-validator` is in that release).
# Local 4.0+ is fine; this machine used 4.0.1.
#
# Usage:
#   scripts/sns-validator.sh          # foreground
#   scripts/sns-validator.sh --bg     # background, wait until RPC is up
#
# Then:
#   SOLANA_RPC=http://127.0.0.1:8899 cargo test -p tangled --test sns_rpc -- --ignored
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$ROOT/crates/tangled/fixtures/sns"
LEDGER="${SNS_LEDGER:-$ROOT/target/sns-test-ledger}"
RPC_PORT="${SOLANA_RPC_PORT:-8899}"
RPC_URL="http://127.0.0.1:${RPC_PORT}"
BG=0

for arg in "$@"; do
  case "$arg" in
    --bg|--background) BG=1 ;;
    -h|--help)
      sed -n '2,14p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if ! command -v solana-test-validator >/dev/null 2>&1; then
  echo "solana-test-validator not found. Install Agave (CI uses v4.2.1):" >&2
  echo "  sh -c \"\$(curl -sSfL https://release.anza.xyz/v4.2.1/install)\"" >&2
  exit 1
fi

if [[ ! -d "$FIXTURES" ]]; then
  echo "missing fixtures directory: $FIXTURES" >&2
  exit 1
fi

mkdir -p "$LEDGER"

wait_rpc() {
  local i
  for i in $(seq 1 90); do
    if curl -sf "$RPC_URL" -X POST -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q ok; then
      echo "solana-test-validator healthy at $RPC_URL"
      return 0
    fi
    if curl -sf "$RPC_URL" -X POST -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}' 2>/dev/null | grep -q result; then
      echo "solana-test-validator ready at $RPC_URL"
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for $RPC_URL" >&2
  echo "validator log: $LEDGER/validator.log" >&2
  return 1
}

# --reset so --account-dir is applied (ignored if a ledger already exists).
# --quiet writes logs under $LEDGER/validator.log rather than a dashboard.
cmd=(
  solana-test-validator
  --reset
  --quiet
  --bind-address 127.0.0.1
  --rpc-port "$RPC_PORT"
  --ledger "$LEDGER"
  --account-dir "$FIXTURES"
  --limit-ledger-size 10000
)

echo "starting: ${cmd[*]}"
if [[ "$BG" -eq 1 ]]; then
  nohup "${cmd[@]}" >"$LEDGER/stdout.log" 2>&1 &
  echo $! >"$LEDGER/validator.pid"
  disown $! 2>/dev/null || true
  echo "pid $(cat "$LEDGER/validator.pid") ledger $LEDGER"
  wait_rpc
else
  exec "${cmd[@]}"
fi
