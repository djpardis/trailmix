#!/usr/bin/env bash
# Quick evaluation loop: build release, run fast+parallel on development corpora.
# Usage: ./benchmarks/quick_eval.sh [--limit N] [corpus...]
# Corpora: key, tempo, fmakv2, fsl10k (default: key)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LIMIT=""
CORPORA=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --limit) LIMIT="--limit $2"; shift 2 ;;
        *) CORPORA+=("$1"); shift ;;
    esac
done

[[ ${#CORPORA[@]} -eq 0 ]] && CORPORA=("key")

cd "$REPO_ROOT"
cargo build -p trailmix-bench --release 2>/dev/null

BENCH="${CARGO_TARGET_DIR:-target}/release/trailmix-bench"

manifest_for() {
    case "$1" in
        key)    echo "benchmarks/giantsteps-key.private.json" ;;
        tempo)  echo "benchmarks/giantsteps-tempo.private.json" ;;
        fmakv2) echo "benchmarks/fmakv2-external.private.json" ;;
        fsl10k) echo "benchmarks/fsl10k-external.private.json" ;;
        *)      echo "" ;;
    esac
}

for corpus in "${CORPORA[@]}"; do
    manifest="$(manifest_for "$corpus")"
    if [[ -z "$manifest" || ! -f "$manifest" ]]; then
        echo "skip: $corpus (manifest not found)"
        continue
    fi
    echo "=== $corpus ==="
    # shellcheck disable=SC2086
    "$BENCH" --manifest "$manifest" --fast --parallel $LIMIT 2>/dev/null \
        | python3 -c "
import sys, json
d = json.load(sys.stdin)
s = d['summary']
parts = [f'tracks={s[\"analyzed_tracks\"]}']
ka = s.get('exact_key_accuracy')
if ka is not None: parts.append(f'key={ka:.1%}')
mx = s.get('mirex_weighted_score')
if mx is not None: parts.append(f'mirex={mx:.3f}')
bpm = s.get('bpm_mean_absolute_error')
if bpm is not None: parts.append(f'bpm_mae={bpm:.2f}')
oct = s.get('bpm_octave_aware_mean_absolute_error')
if oct is not None: parts.append(f'bpm_oct={oct:.2f}')
print('  ' + ' | '.join(parts))
"
done
