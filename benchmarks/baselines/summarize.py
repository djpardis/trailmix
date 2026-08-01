"""Summarize external baseline predictions with shared benchmark metrics."""

from __future__ import annotations

import argparse
import json
import statistics
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

PITCH_CLASSES = {
    "C": 0,
    "C#": 1,
    "D": 2,
    "D#": 3,
    "E": 4,
    "F": 5,
    "F#": 6,
    "G": 7,
    "G#": 8,
    "A": 9,
    "A#": 10,
    "B": 11,
}

FLAT_TO_SHARP = {
    "Db": "C#",
    "Eb": "D#",
    "Gb": "F#",
    "Ab": "G#",
    "Bb": "A#",
}

ACCURACY_2_FACTORS = (1.0, 0.5, 2.0, 1.0 / 3.0, 3.0)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def parse_key(value: str) -> tuple[int, str]:
    tonic, mode = value.split()
    tonic = tonic.replace("♭", "b").replace("♯", "#")
    tonic = FLAT_TO_SHARP.get(tonic, tonic)
    return PITCH_CLASSES[tonic], mode


def key_relationship(reference: str, detected: str) -> tuple[str, float]:
    reference_tonic, reference_mode = parse_key(reference)
    detected_tonic, detected_mode = parse_key(detected)
    difference = (detected_tonic - reference_tonic) % 12

    if (reference_tonic, reference_mode) == (detected_tonic, detected_mode):
        return "exact", 1.0
    if reference_mode == detected_mode and difference in {5, 7}:
        return "fifth", 0.5
    is_relative = reference_mode != detected_mode and (
        (reference_mode == "major" and difference == 9)
        or (reference_mode == "minor" and difference == 3)
    )
    if is_relative:
        return "relative", 0.3
    if reference_mode != detected_mode and difference == 0:
        return "parallel", 0.2
    return "other", 0.0


def tempo_matches(reference: float, detected: float, multiplier: float) -> bool:
    target = reference * multiplier
    return abs(detected - target) / target <= 0.04


def summarize_key(results: list[dict[str, Any]]) -> dict[str, Any]:
    successful = [
        result
        for result in results
        if result.get("error") is None and result.get("detected_key") is not None
    ]
    relationships = [
        key_relationship(result["expected_key"], result["detected_key"])
        for result in successful
    ]
    counts = Counter(relationship for relationship, _ in relationships)
    detected_tonics = Counter(
        result["detected_key"].split()[0] for result in successful
    )
    if not successful:
        return {
            "track_count": len(results),
            "analyzed_tracks": 0,
            "failed_tracks": len(results),
            "exact_accuracy": None,
            "mirex_weighted_score": None,
            "relationships": {},
            "c_prediction_share": None,
            "mean_elapsed_milliseconds": None,
        }
    return {
        "track_count": len(results),
        "analyzed_tracks": len(successful),
        "failed_tracks": len(results) - len(successful),
        "exact_accuracy": counts["exact"] / len(successful),
        "mirex_weighted_score": (
            sum(score for _, score in relationships) / len(successful)
        ),
        "relationships": dict(sorted(counts.items())),
        "c_prediction_share": detected_tonics["C"] / len(successful),
        "mean_elapsed_milliseconds": statistics.fmean(
            result["elapsed_milliseconds"] for result in successful
        ),
    }


def summarize_tempo(results: list[dict[str, Any]]) -> dict[str, Any]:
    successful = [
        result
        for result in results
        if result.get("error") is None and result.get("detected_bpm") is not None
    ]
    if not successful:
        return {
            "track_count": len(results),
            "analyzed_tracks": 0,
            "failed_tracks": len(results),
            "accuracy_1": None,
            "accuracy_2": None,
            "mean_absolute_error_bpm": None,
            "median_absolute_error_bpm": None,
            "mean_elapsed_milliseconds": None,
        }
    absolute_errors = [
        abs(result["detected_bpm"] - result["expected_bpm"]) for result in successful
    ]
    accuracy_1 = sum(
        tempo_matches(result["expected_bpm"], result["detected_bpm"], 1.0)
        for result in successful
    )
    accuracy_2 = sum(
        any(
            tempo_matches(result["expected_bpm"], result["detected_bpm"], multiplier)
            for multiplier in ACCURACY_2_FACTORS
        )
        for result in successful
    )
    return {
        "track_count": len(results),
        "analyzed_tracks": len(successful),
        "failed_tracks": len(results) - len(successful),
        "accuracy_1": accuracy_1 / len(successful),
        "accuracy_2": accuracy_2 / len(successful),
        "mean_absolute_error_bpm": statistics.fmean(absolute_errors),
        "median_absolute_error_bpm": statistics.median(absolute_errors),
        "mean_elapsed_milliseconds": statistics.fmean(
            result["elapsed_milliseconds"] for result in successful
        ),
    }


def main() -> None:
    arguments = parse_arguments()
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for input_path in arguments.input:
        with input_path.open() as source:
            for line in source:
                if line.strip():
                    result = json.loads(line)
                    grouped[result["tool"]].append(result)

    summary = {"version": 1, "tools": {}}
    for tool, results in sorted(grouped.items()):
        first = results[0]
        if first.get("expected_key") is not None:
            metrics = summarize_key(results)
        else:
            metrics = summarize_tempo(results)
        summary["tools"][tool] = {
            "version": first["tool_version"],
            "corpus_role": first.get("corpus_role", "unrecorded"),
            "corpus_fingerprint": first.get("corpus_fingerprint"),
            "model_sha256": first.get("model_sha256"),
            **metrics,
        }

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
