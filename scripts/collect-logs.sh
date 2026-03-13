#!/usr/bin/env bash
# Usage: scripts/collect-logs.sh [seconds]
# Runs the interactive example for N seconds, capturing all stderr output.
set -euo pipefail
cd "$(dirname "$0")/.."

DURATION="${1:-10}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
LOGFILE="debug/logs/run_${TIMESTAMP}.log"
LATEST_LOG="debug/logs/latest.log"

mkdir -p debug/logs

echo "[collect-logs] running for ${DURATION}s, logging to $LOGFILE"

# Run the example in the background, capture output
RUST_LOG="${RUST_LOG:-debug}" cargo run --bin example 2>&1 | tee "$LOGFILE" &
APP_PID=$!

sleep "$DURATION"

# Kill the app and its children
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true

cp "$LOGFILE" "$LATEST_LOG"
echo "[collect-logs] done → $LOGFILE (also at $LATEST_LOG)"
echo "[collect-logs] lines captured: $(wc -l < "$LOGFILE")"
