"""Evaluate key-change boundaries from a trailmix-bench JSON report.

Reference boundaries are transitions between adjacent annotated key segments.
Detected boundaries are transitions between adjacent emitted key segments.
Each reference boundary can match at most one detection within the supplied
tolerance, and each detection can match at most one reference.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def boundaries(segments: list[dict[str, Any]]) -> list[float]:
    values: list[float] = []
    for previous, current in zip(segments, segments[1:]):
        if previous.get("key") and current.get("key") and previous["key"] != current["key"]:
            values.append(float(current["start_seconds"]))
    return values


def match_boundaries(
    reference: list[float],
    detected: list[float],
    tolerance: float,
) -> tuple[int, int, int]:
    matched: set[int] = set()
    true_positives = 0
    for reference_time in reference:
        candidates = [
            (abs(detected_time - reference_time), index)
            for index, detected_time in enumerate(detected)
            if index not in matched and abs(detected_time - reference_time) <= tolerance
        ]
        if candidates:
            _, index = min(candidates)
            matched.add(index)
            true_positives += 1
    return true_positives, len(detected) - true_positives, len(reference) - true_positives


def evaluate(
    manifest: dict[str, Any],
    report: dict[str, Any],
    tolerance: float,
) -> dict[str, Any]:
    references = {
        track["id"]: boundaries(track.get("expected_key_segments", []))
        for track in manifest["tracks"]
    }
    detections = {
        track["id"]: boundaries(track.get("detected_key_segments", []))
        for track in report["tracks"]
    }

    counts = {"true_positives": 0, "false_positives": 0, "false_negatives": 0}
    evaluated_tracks = 0
    failed_tracks = 0
    per_track: list[dict[str, Any]] = []

    for track_id, reference in references.items():
        result = report["tracks"][next(
            index for index, track in enumerate(report["tracks"]) if track["id"] == track_id
        )]
        if result.get("error"):
            failed_tracks += 1
            continue
        detected = detections.get(track_id, [])
        tp, fp, fn = match_boundaries(reference, detected, tolerance)
        evaluated_tracks += 1
        for key, value in (("true_positives", tp), ("false_positives", fp), ("false_negatives", fn)):
            counts[key] += value
        per_track.append({
            "id": track_id,
            "reference_boundaries": reference,
            "detected_boundaries": detected,
            "true_positives": tp,
            "false_positives": fp,
            "false_negatives": fn,
        })

    precision = counts["true_positives"] / (
        counts["true_positives"] + counts["false_positives"]
    ) if counts["true_positives"] + counts["false_positives"] else 0.0
    recall = counts["true_positives"] / (
        counts["true_positives"] + counts["false_negatives"]
    ) if counts["true_positives"] + counts["false_negatives"] else 0.0
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0

    return {
        "tolerance_seconds": tolerance,
        "evaluated_tracks": evaluated_tracks,
        "failed_tracks": failed_tracks,
        **counts,
        "precision": precision,
        "recall": recall,
        "f1": f1,
        "per_track": per_track,
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--tolerance", type=float, default=3.0)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = parse_arguments()
    result = evaluate(
        json.loads(arguments.manifest.read_text()),
        json.loads(arguments.report.read_text()),
        arguments.tolerance,
    )
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if arguments.output:
        arguments.output.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
