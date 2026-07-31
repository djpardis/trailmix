"""Compare two trailmix-bench result files and print a regression table."""

import argparse
import json
import sys
from pathlib import Path

METRICS = [
    ("exact_key_accuracy", "Key exact accuracy", "higher", "{:.1%}"),
    ("mirex_weighted_score", "MIREX weighted score", "higher", "{:.3f}"),
    ("bpm_mean_absolute_error", "BPM MAE", "lower", "{:.2f}"),
    ("bpm_octave_aware_mean_absolute_error", "BPM octave-aware MAE", "lower", "{:.2f}"),
    ("beat_f1", "Beat F1", "higher", "{:.3f}"),
    ("multi_key_tracks", "Multi-key tracks", "info", "{}"),
    ("multi_tempo_tracks", "Multi-tempo tracks", "info", "{}"),
    ("mean_analysis_milliseconds", "Mean analysis (ms)", "lower", "{:.1f}"),
    ("serato_exact_key_agreement", "Serato key agreement", "higher", "{:.1%}"),
]


def load_summary(path: Path) -> dict:
    with path.open() as f:
        data = json.load(f)
    return data.get("summary", data)


def format_delta(baseline_val, current_val, direction, fmt):
    if baseline_val is None or current_val is None:
        return ""
    delta = current_val - baseline_val
    if abs(delta) < 1e-9:
        return "="
    if "%" in fmt:
        delta_str = f"{delta:+.1%}"
    elif "f" in fmt:
        delta_str = f"{delta:+.2f}"
    else:
        delta_str = f"{delta:+g}"

    if direction == "higher":
        indicator = "+" if delta > 0 else "REGRESSION"
    elif direction == "lower":
        indicator = "+" if delta < 0 else "REGRESSION"
    else:
        indicator = ""

    return f"{delta_str} {indicator}".strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Baseline result JSON")
    parser.add_argument("current", type=Path, help="Current result JSON")
    args = parser.parse_args()

    if not args.baseline.exists():
        sys.exit(f"baseline not found: {args.baseline}")
    if not args.current.exists():
        sys.exit(f"current not found: {args.current}")

    baseline = load_summary(args.baseline)
    current = load_summary(args.current)

    print(f"{'Metric':<28} {'Baseline':>12} {'Current':>12} {'Delta':>18}")
    print("-" * 72)

    for key, label, direction, fmt in METRICS:
        b_val = baseline.get(key)
        c_val = current.get(key)
        b_str = fmt.format(b_val) if b_val is not None else "-"
        c_str = fmt.format(c_val) if c_val is not None else "-"
        delta = format_delta(b_val, c_val, direction, fmt)
        print(f"{label:<28} {b_str:>12} {c_str:>12} {delta:>18}")


if __name__ == "__main__":
    main()
