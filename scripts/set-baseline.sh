#!/usr/bin/env bash
# Usage: scripts/set-baseline.sh [source.png]
# Promotes a frame to the baseline for future comparisons.
# If no source is given, renders a fresh frame first.
set -euo pipefail
cd "$(dirname "$0")/.."

SOURCE="${1:-}"
BASELINE="debug/baseline/baseline.png"

mkdir -p debug/baseline

if [ -z "$SOURCE" ]; then
    echo "[set-baseline] no source given, rendering fresh frame..."
    bash scripts/dump-frame.sh debug/baseline/baseline_new.png
    SOURCE="debug/baseline/baseline_new.png"
fi

cp "$SOURCE" "$BASELINE"
echo "[set-baseline] baseline set from: $SOURCE → $BASELINE"
