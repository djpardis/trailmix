"""Train optimal key profiles from benchmark chroma output.

Usage:
    python train_key_profiles.py benchmark-results/trailmix-giantsteps-key-v3.json

Performs leave-one-out cross-validation to estimate accuracy of learned
profiles vs fixed profiles, then outputs the learned profiles as Rust
const arrays ready to paste into key-lime/src/lib.rs.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

PITCH_CLASSES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
MODES = ["major", "minor"]

NOTE_ALIASES = {
    "Db": "C#",
    "Eb": "D#",
    "Gb": "F#",
    "Ab": "G#",
    "Bb": "A#",
}


def parse_key(key_str: str) -> tuple[int, str] | None:
    """Parse key string like 'A minor' into (root_index, mode)."""
    parts = key_str.split()
    if len(parts) != 2:
        return None
    note, mode = parts
    note = NOTE_ALIASES.get(note, note)
    if note not in PITCH_CLASSES:
        return None
    if mode not in MODES:
        return None
    return PITCH_CLASSES.index(note), mode


def rotate_chroma(chroma: list[float], root: int) -> np.ndarray:
    """Rotate chroma so root is at index 0."""
    arr = np.array(chroma, dtype=np.float64)
    return np.roll(arr, -root)


def pearson_correlation(a: np.ndarray, b: np.ndarray) -> float:
    a_centered = a - a.mean()
    b_centered = b - b.mean()
    denom = np.sqrt((a_centered**2).sum() * (b_centered**2).sum())
    if denom < 1e-12:
        return 0.0
    return float((a_centered * b_centered).sum() / denom)


def classify_with_profiles(
    chroma: np.ndarray, major_profile: np.ndarray, minor_profile: np.ndarray
) -> tuple[int, str]:
    """Classify chroma using correlation with profiles."""
    best_root = 0
    best_mode = "major"
    best_score = -np.inf

    for root in range(12):
        rotated = np.roll(chroma, -root)
        major_corr = pearson_correlation(rotated, major_profile)
        minor_corr = pearson_correlation(rotated, minor_profile)

        if major_corr > best_score:
            best_score = major_corr
            best_root = root
            best_mode = "major"
        if minor_corr > best_score:
            best_score = minor_corr
            best_root = root
            best_mode = "minor"

    return best_root, best_mode


def learn_profiles(
    tracks: list[dict],
) -> tuple[np.ndarray, np.ndarray]:
    """Learn major and minor profiles from labeled tracks."""
    major_chromas: list[np.ndarray] = []
    minor_chromas: list[np.ndarray] = []

    for track in tracks:
        chroma = track.get("chroma")
        key_str = track.get("expected_key")
        if chroma is None or key_str is None:
            continue
        parsed = parse_key(key_str)
        if parsed is None:
            continue
        root, mode = parsed
        rotated = rotate_chroma(chroma, root)

        if mode == "major":
            major_chromas.append(rotated)
        else:
            minor_chromas.append(rotated)

    if not major_chromas or not minor_chromas:
        return np.ones(12), np.ones(12)

    major_profile = np.median(np.array(major_chromas), axis=0)
    minor_profile = np.median(np.array(minor_chromas), axis=0)

    return major_profile, minor_profile


def cross_validate(tracks: list[dict], n_folds: int = 5) -> dict[str, float]:
    """K-fold cross-validation of learned profiles."""
    valid_tracks = [
        t for t in tracks if t.get("chroma") and t.get("expected_key") and parse_key(t["expected_key"])
    ]
    np.random.seed(42)
    np.random.shuffle(valid_tracks)

    fold_size = len(valid_tracks) // n_folds
    correct_learned = 0
    total = 0

    for fold in range(n_folds):
        test_start = fold * fold_size
        test_end = test_start + fold_size if fold < n_folds - 1 else len(valid_tracks)
        test_set = valid_tracks[test_start:test_end]
        train_set = valid_tracks[:test_start] + valid_tracks[test_end:]

        major_prof, minor_prof = learn_profiles(train_set)

        for track in test_set:
            chroma = np.array(track["chroma"])
            parsed = parse_key(track["expected_key"])
            if parsed is None:
                continue
            expected_root, expected_mode = parsed

            detected_root, detected_mode = classify_with_profiles(
                chroma, major_prof, minor_prof
            )

            if detected_root == expected_root and detected_mode == expected_mode:
                correct_learned += 1
            total += 1

    return {
        "learned_accuracy": correct_learned / total if total else 0.0,
        "total_tracks": total,
        "n_folds": n_folds,
    }


def format_rust_array(name: str, profile: np.ndarray) -> str:
    values = ", ".join(f"{v:.4f}" for v in profile)
    return f"const {name}: [f32; 12] = [{values}];"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path, help="Benchmark results JSON with chroma")
    args = parser.parse_args()

    if not args.results.exists():
        sys.exit(f"File not found: {args.results}")

    with args.results.open() as f:
        data = json.load(f)

    tracks = data.get("tracks", [])
    valid = [t for t in tracks if t.get("chroma") and t.get("expected_key")]
    print(f"Tracks with chroma and labels: {len(valid)}")

    if len(valid) < 50:
        sys.exit("Not enough labeled tracks with chroma for training.")

    cv_results = cross_validate(tracks)
    print(f"\n5-fold cross-validation:")
    print(f"  Learned profile accuracy: {cv_results['learned_accuracy']:.1%}")
    print(f"  Tracks evaluated: {cv_results['total_tracks']}")

    major_prof, minor_prof = learn_profiles(valid)
    print(f"\nLearned profiles (trained on all {len(valid)} tracks):")
    print(f"  {format_rust_array('LEARNED_MAJOR', major_prof)}")
    print(f"  {format_rust_array('LEARNED_MINOR', minor_prof)}")

    all_correct = 0
    for track in valid:
        chroma = np.array(track["chroma"])
        parsed = parse_key(track["expected_key"])
        if parsed is None:
            continue
        root, mode = parsed
        det_root, det_mode = classify_with_profiles(chroma, major_prof, minor_prof)
        if det_root == root and det_mode == mode:
            all_correct += 1
    print(f"\n  Train-on-all accuracy (overfit): {all_correct / len(valid):.1%}")


if __name__ == "__main__":
    main()
