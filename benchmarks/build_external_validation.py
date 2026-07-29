"""Build deterministic FMAKv2 and FSL10K external-validation manifests."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any


def stable_sample(
    items: list[dict[str, Any]], namespace: str, count: int
) -> list[dict[str, Any]]:
    if count < 1:
        raise ValueError("sample count must be at least 1")
    ranked = sorted(
        items,
        key=lambda item: hashlib.sha256(f"{namespace}:{item['id']}".encode()).digest(),
    )
    return sorted(ranked[:count], key=lambda item: item["id"])


def build_fmakv2(
    annotation_path: Path, audio_directory: Path, count: int
) -> dict[str, Any]:
    tracks = []
    with annotation_path.open(newline="") as stream:
        for row in csv.DictReader(stream):
            track_number = int(row["track_id"])
            key, mode = row["key_and_mode"].split()
            tracks.append(
                {
                    "id": f"fmakv2-{track_number:06d}",
                    "path": str(
                        audio_directory
                        / f"{track_number // 1000:03d}"
                        / f"{track_number:06d}.mp3"
                    ),
                    "split": "external-validation",
                    "source": {
                        "name": "FMAKv2",
                        "version": "Zenodo 12759100",
                        "item_id": str(track_number),
                        "citation": "Kong et al., STONE, ISMIR 2024; Wong and Hernandez, FMAK, ISMIR 2023.",
                    },
                    "expected_key": f"{key} {mode.lower()}",
                    "annotation": {
                        "status": "expert",
                        "reviewer": "FMAKv2",
                        "notes": "Imported from the CC BY 4.0 FMAKv2 annotation release.",
                    },
                }
            )
    return {"version": 1, "tracks": stable_sample(tracks, "fmakv2", count)}


def build_fsl10k(
    annotation_directory: Path,
    audio_directory: Path,
    count: int,
) -> dict[str, Any]:
    annotations: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for path in annotation_directory.glob("*/*.json"):
        annotations[path.stem.removeprefix("sound-")].append(
            json.loads(path.read_text())
        )

    tracks = []
    for sound_id, rows in annotations.items():
        bpms = []
        for row in rows:
            if row.get("discard") or not row.get("well_cut"):
                continue
            try:
                bpms.append(float(row["bpm"]))
            except (KeyError, TypeError, ValueError):
                continue
        if len(bpms) < 2 or max(bpms) - min(bpms) > 1.0:
            continue
        tracks.append(
            {
                "id": f"fsl10k-{int(sound_id):06d}",
                "path": str(audio_directory / f"{sound_id}.wav"),
                "split": "external-validation",
                "source": {
                    "name": "FSL10K",
                    "version": "1.0",
                    "item_id": sound_id,
                    "citation": "Ramires et al., The Freesound Loop Dataset and Annotation Tool, ISMIR 2020.",
                },
                "expected_bpm": statistics.median(bpms),
                "annotation": {
                    "status": "consensus",
                    "reviewer": "FSL10K",
                    "notes": f"Median of {len(bpms)} annotations agreeing within 1 BPM.",
                },
            }
        )
    return {"version": 1, "tracks": stable_sample(tracks, "fsl10k", count)}


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fmakv2-csv", type=Path, required=True)
    parser.add_argument("--fmakv2-audio", type=Path, required=True)
    parser.add_argument("--fmakv2-output", type=Path, required=True)
    parser.add_argument("--fsl10k-annotations", type=Path, required=True)
    parser.add_argument("--fsl10k-audio", type=Path, required=True)
    parser.add_argument("--fsl10k-output", type=Path, required=True)
    parser.add_argument("--sample-count", type=int, default=1_000)
    return parser.parse_args()


def write_manifest(path: Path, manifest: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


def main() -> None:
    arguments = parse_arguments()
    write_manifest(
        arguments.fmakv2_output,
        build_fmakv2(
            arguments.fmakv2_csv,
            arguments.fmakv2_audio,
            arguments.sample_count,
        ),
    )
    write_manifest(
        arguments.fsl10k_output,
        build_fsl10k(
            arguments.fsl10k_annotations,
            arguments.fsl10k_audio,
            arguments.sample_count,
        ),
    )


if __name__ == "__main__":
    main()
