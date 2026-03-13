#!/usr/bin/env bash
# Usage: scripts/compare-frames.sh [baseline.png] [current.png] [diff.png]
# Compares two frames and reports PSNR / pixel difference.
# Requires ImageMagick. Falls back to Python if unavailable.
set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE="${1:-debug/baseline/baseline.png}"
CURRENT="${2:-debug/frames/latest.png}"
DIFF_OUT="${3:-debug/frames/diff.png}"

if [ ! -f "$BASELINE" ]; then
    echo "ERROR: baseline not found: $BASELINE" >&2
    echo "Run scripts/set-baseline.sh first." >&2
    exit 1
fi
if [ ! -f "$CURRENT" ]; then
    echo "ERROR: current frame not found: $CURRENT" >&2
    echo "Run scripts/dump-frame.sh first." >&2
    exit 1
fi

mkdir -p "$(dirname "$DIFF_OUT")"

if command -v compare &>/dev/null; then
    echo "[compare-frames] using ImageMagick"
    PSNR=$(compare -metric PSNR "$BASELINE" "$CURRENT" "$DIFF_OUT" 2>&1 || true)
    echo "[compare-frames] PSNR: $PSNR"
    echo "[compare-frames] diff written to: $DIFF_OUT"
elif command -v python3 &>/dev/null; then
    echo "[compare-frames] ImageMagick not found, using Python (install python3-pil for best results)"
    python3 - "$BASELINE" "$CURRENT" "$DIFF_OUT" <<'PYEOF'
import sys, struct, zlib

def load_png_pixels(path):
    """Minimal PNG RGBA reader - delegates to PIL if available."""
    try:
        from PIL import Image, ImageChops
        return Image.open(path).convert("RGBA")
    except ImportError:
        pass
    raise RuntimeError("Install Pillow: pip3 install Pillow")

try:
    from PIL import Image, ImageChops, ImageFilter
    import math

    baseline = Image.open(sys.argv[1]).convert("RGBA")
    current  = Image.open(sys.argv[2]).convert("RGBA")

    if baseline.size != current.size:
        print(f"[compare-frames] WARNING: size mismatch {baseline.size} vs {current.size}")

    diff = ImageChops.difference(baseline, current)

    # Amplify diff for visibility
    amplified = diff.point(lambda p: min(255, p * 8))
    amplified.save(sys.argv[3])

    import numpy as np
    b = np.array(baseline, dtype=float)
    c = np.array(current,  dtype=float)
    mse = ((b - c) ** 2).mean()
    psnr = 10 * math.log10(255**2 / mse) if mse > 0 else float("inf")
    max_diff = int(np.abs(b - c).max())
    changed_pixels = int((np.abs(b - c).sum(axis=2) > 0).sum())

    print(f"[compare-frames] PSNR:           {psnr:.2f} dB")
    print(f"[compare-frames] Max pixel delta: {max_diff}")
    print(f"[compare-frames] Changed pixels:  {changed_pixels} / {baseline.size[0]*baseline.size[1]}")
    print(f"[compare-frames] diff written to: {sys.argv[3]}")

except Exception as e:
    print(f"[compare-frames] ERROR: {e}")
    sys.exit(1)
PYEOF
else
    echo "ERROR: neither ImageMagick nor Python3 found." >&2
    exit 1
fi
