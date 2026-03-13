#!/usr/bin/env bash
# Usage: scripts/run.sh [release]
# Starts the interactive example (live window).
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${1:-debug}"
RUST_LOG="${RUST_LOG:-info}"

if [ "$PROFILE" = "release" ]; then
    RUST_LOG="$RUST_LOG" cargo run --release --bin example 2>&1
else
    RUST_LOG="$RUST_LOG" cargo run --bin example 2>&1
fi
