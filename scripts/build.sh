#!/usr/bin/env bash
# Usage: scripts/build.sh [release]
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${1:-debug}"

if [ "$PROFILE" = "release" ]; then
    cargo build --release 2>&1
else
    cargo build 2>&1
fi
