#!/usr/bin/env bash
# Usage: scripts/dump-frame.sh [output.png]
# Renders one deterministic frame of the debug scene and saves it as a PNG.
# The scene uses a fixed camera and basicmesh.glb.
set -euo pipefail
cd "$(dirname "$0")/.."

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
OUTPUT="${1:-debug/frames/frame_${TIMESTAMP}.png}"

mkdir -p "$(dirname "$OUTPUT")"

echo "[dump-frame] rendering to: $OUTPUT"
LANTIR_DUMP_FRAME="$OUTPUT" RUST_LOG="${RUST_LOG:-warn}" cargo run --bin debug_scene 2>&1

# Also update the "latest" symlink for easy reference
LATEST="debug/frames/latest.png"
cp "$OUTPUT" "$LATEST"
echo "[dump-frame] done → $OUTPUT (also copied to $LATEST)"
