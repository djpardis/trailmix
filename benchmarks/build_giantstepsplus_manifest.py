"""Build a GiantSteps+ segment evaluation manifest from Faraldo (2017) annotations.

The GiantSteps+ dataset extends GiantSteps Key (604 tracks) with reannotated keys
and segment-level key change annotations for 600 EDM two-minute previews.

Annotation format (per-track JSON):
  key         list of key labels, one per segment
  start_times list of segment start positions in centiseconds
  end_times   list of segment end positions; the last value is always 120 (seconds),
              marking the track end; all others are in centiseconds

Audio filenames match the existing GiantSteps Key download:
  <id>.LOFI.mp3

Usage:
  uv run --project benchmarks/baselines python benchmarks/build_giantstepsplus_manifest.py \\
    --annotations <path-to-new_annotations-dir> \\
    --output benchmarks/giantsteps-plus.private.json
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

AUDIO_DIR = "local/datasets/giantsteps-key/audio"
TRACK_DURATION_SECONDS = 120.0

# Map of underscore/caret shorthand to standard flat names.
_ROOT_FIXES: dict[str, str] = {
    "Ab": "Ab",
    "Bb": "Bb",
    "Cb": "Cb",
    "Db": "Db",
    "Eb": "Eb",
    "Fb": "Fb",
    "Gb": "Gb",
    "A": "A",
    "B": "B",
    "C": "C",
    "D": "D",
    "E": "E",
    "F": "F",
    "G": "G",
    "C#": "C#",
    "F#": "F#",
    # Underscore notation: D_ = Db, E_ = Eb, B_ = Bb.
    "D_": "Db",
    "E_": "Eb",
    "B_": "Bb",
    # Caret notation: Eb^ = Eb.
    "Eb^": "Eb",
    "Ab^": "Ab",
}

# Mode qualifiers that map to "major" or "minor".
_MODE_MAJOR = {
    "major", "major ionian", "major mixolydian", "major lydian",
    "major aeolian", "major phrygian-major", "major dorian",
}
_MODE_MINOR = {
    "minor", "minor aeolian", "minor dorian", "minor harmonic",
    "minor melodic", "minor phrygian", "minor phygian", "minor phryrgian",
    "minor mixolydian",
}


def normalize_key(label: str) -> str | None:
    """Return 'X major' or 'X minor', or None if the label cannot be normalized."""
    # Take the primary interpretation if the annotator gave alternatives.
    primary = label.split("|")[0].strip()
    parts = primary.split(maxsplit=1)
    if len(parts) < 2:
        return None
    root_raw, mode_part = parts[0].strip(), parts[1].strip().lower()
    root = _ROOT_FIXES.get(root_raw)
    if root is None:
        # Try stripping trailing punctuation.
        root = _ROOT_FIXES.get(re.sub(r"[^A-Za-z0-9#b_^]", "", root_raw))
    if root is None:
        return None
    if mode_part in _MODE_MAJOR:
        return f"{root} major"
    if mode_part in _MODE_MINOR:
        return f"{root} minor"
    return None


def centiseconds_to_seconds(cs: int | float) -> float:
    return round(cs / 100.0, 3)


def build_track(
    track_id: str,
    annotation: dict[str, Any],
) -> dict[str, Any] | None:
    raw_keys: list[str] = annotation.get("key") or []
    start_times: list[int | float] = annotation.get("start_times") or [0]
    end_times: list[int | float] = annotation.get("end_times") or [120]

    if not raw_keys:
        return None

    # Resolve the key label for the first segment as the global expected key.
    primary_key = normalize_key(raw_keys[0])

    # Build segment list.
    segments: list[dict[str, Any]] = []
    for i, key_label in enumerate(raw_keys):
        start_s = centiseconds_to_seconds(start_times[i]) if i < len(start_times) else 0.0
        if i == len(raw_keys) - 1:
            # Last segment always ends at the track duration.
            end_s = TRACK_DURATION_SECONDS
        else:
            end_s = centiseconds_to_seconds(end_times[i])
        canonical = normalize_key(key_label)
        segments.append(
            {
                "start_seconds": start_s,
                "end_seconds": end_s,
                "key": canonical,
                "raw_label": key_label,
                "normalizable": canonical is not None,
            }
        )

    # Normalize annotator confidence (0-3 ordinal) to (0, 1] range expected by the bench.
    raw_confidence = annotation.get("confidence") or 0
    segment_confidence = max(0.25, min(1.0, (raw_confidence + 1) / 4.0))

    # Build change events for tracks with more than one key segment.
    change_events: list[dict[str, Any]] = []
    for seg in segments[1:]:
        change_events.append(
            {
                "time_seconds": seg["start_seconds"],
                "key_changed": True,
                "tempo_changed": False,
                "confidence": segment_confidence,
                "label": "Key change",
            }
        )

    # Only include expected_key_segments when every segment has a normalizable key.
    all_normalizable = all(seg["normalizable"] for seg in segments)
    expected_key_segments = (
        [
            {
                "start_seconds": seg["start_seconds"],
                "end_seconds": seg["end_seconds"],
                "key": seg["key"],
                "confidence": segment_confidence,
            }
            for seg in segments
        ]
        if all_normalizable
        else None
    )

    track: dict[str, Any] = {
        "id": f"giantsteps-plus-{track_id}",
        "path": f"{AUDIO_DIR}/{track_id}.LOFI.mp3",
        "split": "evaluation",
        "source": {
            "name": "GiantSteps+",
            "version": "Zenodo 4153506",
            "item_id": track_id,
            "citation": (
                "Faraldo, Tonality Estimation in Electronic Dance Music, "
                "Ph.D. thesis, Universitat Pompeu Fabra, 2017."
            ),
        },
        "annotation": {
            "status": "reviewed",
            "reviewer": "Faraldo (2017)",
            "notes": (
                f"Expert re-annotation. Annotator confidence: "
                f"{annotation.get('confidence', 'unknown')}."
                + (f" Comments: {annotation['comments']}" if annotation.get("comments") else "")
            ),
        },
        "multi_key_reference": len(raw_keys) > 1,
    }
    if expected_key_segments is not None:
        track["expected_key_segments"] = expected_key_segments
    if primary_key is not None:
        track["expected_key"] = primary_key
    if change_events:
        track["change_events"] = change_events

    return track


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--annotations",
        type=Path,
        required=True,
        help="Path to the new_annotations directory from the GiantSteps+ Zenodo record.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Output manifest path.",
    )
    return parser.parse_args()


def main() -> None:
    arguments = parse_arguments()
    ann_dir = arguments.annotations

    tracks: list[dict[str, Any]] = []
    skipped = 0

    for path in sorted(ann_dir.glob("*.json")):
        track_id = path.stem
        annotation = json.loads(path.read_text())
        track = build_track(track_id, annotation)
        if track is None:
            skipped += 1
            continue
        # Only include tracks whose audio is expected at the GiantSteps Key path.
        tracks.append(track)

    manifest: dict[str, Any] = {
        "version": 1,
        "corpus": "GiantSteps+",
        "description": (
            "GiantSteps Key dataset reannotated by Faraldo (2017) with expert "
            "segment-level key and key-change annotations. 600 two-minute EDM "
            "previews; 23 tracks carry multi-key annotations."
        ),
        "tracks": tracks,
    }

    multi_key_count = sum(1 for t in tracks if t.get("multi_key_reference"))
    normalizable = sum(1 for t in tracks if t.get("expected_key") is not None)

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    print(
        f"Wrote {len(tracks)} tracks to {arguments.output} "
        f"({multi_key_count} multi-key, {normalizable} with normalizable global key, "
        f"{skipped} skipped)."
    )


if __name__ == "__main__":
    main()
