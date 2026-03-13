#!/usr/bin/env bash
# Kills any running lantir process (example or debug_scene).
set -euo pipefail

KILLED=0
for PATTERN in "target/debug/example" "target/release/example" "target/debug/debug_scene"; do
    if pkill -f "$PATTERN" 2>/dev/null; then
        echo "[stop] killed: $PATTERN"
        KILLED=1
    fi
done

if [ "$KILLED" -eq 0 ]; then
    echo "[stop] no running lantir process found"
fi
