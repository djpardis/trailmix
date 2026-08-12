#!/usr/bin/env bash
# Build the release CLI and print its binary size. Backs the "small binary"
# claim in README with a reproducible number instead of an assertion.
#
# The trailmix-cli binary includes the Symphonia codecs (MP3, FLAC, WAV, AIFF,
# MP4), so it is the full runnable tool, not the analyzers alone.
#
# Usage:
#   ./scripts/measure-binary-size.sh            # build and print sizes only
#   ./scripts/measure-binary-size.sh 3000000    # also fail if unstripped > 3 MB
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUDGET="${1:-0}"  # 0 means print only, do not enforce a budget
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BIN="$TARGET_DIR/release/trailmix-cli"

echo "Building trailmix-cli in release..."
cargo build --release -p trailmix-cli --locked

[ -f "$BIN" ] || { echo "error: release binary not found at $BIN" >&2; exit 1; }

RAW=$(wc -c < "$BIN" | tr -d ' ')

# Report a stripped size too, without altering the build artifact.
STRIPPED_COPY="$TARGET_DIR/release/trailmix-cli.stripped"
cp "$BIN" "$STRIPPED_COPY"
strip "$STRIPPED_COPY" 2>/dev/null || true
STRIPPED=$(wc -c < "$STRIPPED_COPY" | tr -d ' ')
rm -f "$STRIPPED_COPY"

human() { awk -v b="$1" 'BEGIN{ printf "%.1f MB", b / 1000000 }'; }

echo ""
echo "trailmix-cli release binary (includes Symphonia codecs):"
echo "  unstripped: $RAW bytes ($(human "$RAW"))"
echo "  stripped:   $STRIPPED bytes ($(human "$STRIPPED"))"

if [ "$BUDGET" -gt 0 ]; then
  echo "  budget:     $BUDGET bytes ($(human "$BUDGET"))"
  if [ "$RAW" -gt "$BUDGET" ]; then
    echo "FAIL: unstripped binary exceeds the budget." >&2
    exit 1
  fi
  echo "OK: within budget."
fi
