"""Train a tiny MLP key classifier on chroma features from trailmix-bench.

Usage:
    python train_mlp_key.py benchmark-results/trailmix-giantsteps-key-with-chroma.json

Trains a 12 -> 48 -> 24 MLP with cross-validation, then exports
learned weights as Rust const arrays for embedding in key-lime.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np

PITCH_CLASSES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
MODES = ["major", "minor"]
NOTE_ALIASES = {"Db": "C#", "Eb": "D#", "Gb": "F#", "Ab": "G#", "Bb": "A#"}
NUM_CLASSES = 24


def parse_key(key_str: str) -> int | None:
    """Parse key string to class index 0-23 (0=C major, 1=C minor, ..., 23=B minor)."""
    parts = key_str.split()
    if len(parts) != 2:
        return None
    note, mode = parts
    note = NOTE_ALIASES.get(note, note)
    if note not in PITCH_CLASSES or mode not in MODES:
        return None
    root = PITCH_CLASSES.index(note)
    return root * 2 + MODES.index(mode)


def class_to_key(class_idx: int) -> str:
    root = class_idx // 2
    mode = class_idx % 2
    return f"{PITCH_CLASSES[root]} {MODES[mode]}"


def relu(x: np.ndarray) -> np.ndarray:
    return np.maximum(0, x)


def softmax(x: np.ndarray) -> np.ndarray:
    e = np.exp(x - x.max(axis=-1, keepdims=True))
    return e / e.sum(axis=-1, keepdims=True)


class TinyMLP:
    """12 -> hidden_size -> 24 MLP with ReLU activation."""

    def __init__(self, hidden_size: int = 48, learning_rate: float = 0.01):
        self.hidden_size = hidden_size
        self.lr = learning_rate
        self.w1 = np.random.randn(12, hidden_size).astype(np.float32) * 0.1
        self.b1 = np.zeros(hidden_size, dtype=np.float32)
        self.w2 = np.random.randn(hidden_size, NUM_CLASSES).astype(np.float32) * 0.1
        self.b2 = np.zeros(NUM_CLASSES, dtype=np.float32)

    def forward(self, x: np.ndarray) -> np.ndarray:
        self.z1 = x @ self.w1 + self.b1
        self.a1 = relu(self.z1)
        self.z2 = self.a1 @ self.w2 + self.b2
        return softmax(self.z2)

    def predict(self, x: np.ndarray) -> np.ndarray:
        return self.forward(x).argmax(axis=-1)

    def train_step(self, x: np.ndarray, y: np.ndarray) -> float:
        batch_size = x.shape[0]
        probs = self.forward(x)

        one_hot = np.zeros_like(probs)
        one_hot[np.arange(batch_size), y] = 1.0
        loss = -np.mean(np.log(probs[np.arange(batch_size), y] + 1e-8))

        dz2 = (probs - one_hot) / batch_size
        dw2 = self.a1.T @ dz2
        db2 = dz2.sum(axis=0)

        da1 = dz2 @ self.w2.T
        dz1 = da1 * (self.z1 > 0).astype(np.float32)
        dw1 = x.T @ dz1
        db1 = dz1.sum(axis=0)

        self.w1 -= self.lr * dw1
        self.b1 -= self.lr * db1
        self.w2 -= self.lr * dw2
        self.b2 -= self.lr * db2

        return loss

    def fit(self, x: np.ndarray, y: np.ndarray, epochs: int = 200, verbose: bool = False):
        for epoch in range(epochs):
            indices = np.random.permutation(len(x))
            batch_size = 32
            epoch_loss = 0.0
            n_batches = 0
            for start in range(0, len(x), batch_size):
                batch_idx = indices[start : start + batch_size]
                loss = self.train_step(x[batch_idx], y[batch_idx])
                epoch_loss += loss
                n_batches += 1
            if verbose and (epoch + 1) % 50 == 0:
                acc = (self.predict(x) == y).mean()
                print(f"  epoch {epoch+1:3d}: loss={epoch_loss/n_batches:.4f} train_acc={acc:.1%}")


def load_tracks(path: Path) -> tuple[np.ndarray, np.ndarray]:
    """Load chroma vectors and labels from benchmark JSON."""
    with path.open() as f:
        data = json.load(f)

    chromas = []
    labels = []
    for track in data.get("tracks", []):
        chroma = track.get("chroma")
        key_str = track.get("expected_key")
        if chroma is None or key_str is None:
            continue
        label = parse_key(key_str)
        if label is None:
            continue
        chromas.append(chroma)
        labels.append(label)

    return np.array(chromas, dtype=np.float32), np.array(labels, dtype=np.int64)


def cross_validate(x: np.ndarray, y: np.ndarray, n_folds: int = 5) -> dict:
    """K-fold cross-validation."""
    np.random.seed(42)
    indices = np.random.permutation(len(x))
    fold_size = len(x) // n_folds

    accuracies = []
    for fold in range(n_folds):
        test_start = fold * fold_size
        test_end = test_start + fold_size if fold < n_folds - 1 else len(x)
        test_idx = indices[test_start:test_end]
        train_idx = np.concatenate([indices[:test_start], indices[test_end:]])

        model = TinyMLP(hidden_size=48, learning_rate=0.005)
        model.fit(x[train_idx], y[train_idx], epochs=300)
        preds = model.predict(x[test_idx])
        acc = (preds == y[test_idx]).mean()
        accuracies.append(acc)
        print(f"  Fold {fold+1}: {acc:.1%}")

    return {
        "mean_accuracy": float(np.mean(accuracies)),
        "std_accuracy": float(np.std(accuracies)),
        "per_fold": [float(a) for a in accuracies],
    }


def format_rust_weights(name: str, arr: np.ndarray) -> str:
    """Format a weight matrix as a Rust const array."""
    flat = arr.flatten()
    values = ", ".join(f"{v:.6f}" for v in flat)
    shape = "x".join(str(d) for d in arr.shape)
    return f"// Shape: {shape}\nconst {name}: &[f32] = &[{values}];"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path, help="Benchmark results JSON with chroma")
    parser.add_argument("--output", type=Path, default=None, help="Output Rust weights file")
    args = parser.parse_args()

    if not args.results.exists():
        sys.exit(f"File not found: {args.results}")

    x, y = load_tracks(args.results)
    print(f"Loaded {len(x)} tracks with chroma and labels")
    print(f"Class distribution: {np.bincount(y, minlength=24)}")

    if len(x) < 50:
        sys.exit("Not enough labeled tracks for training.")

    print(f"\n5-fold cross-validation (12->48->24 MLP):")
    cv = cross_validate(x, y)
    print(f"\n  Mean accuracy: {cv['mean_accuracy']:.1%} (+/- {cv['std_accuracy']:.1%})")
    print(f"  (Profile-based baseline: ~34%)")

    print(f"\nTraining final model on all {len(x)} tracks...")
    final_model = TinyMLP(hidden_size=48, learning_rate=0.005)
    final_model.fit(x, y, epochs=500, verbose=True)
    train_acc = (final_model.predict(x) == y).mean()
    print(f"  Final train accuracy: {train_acc:.1%}")

    if args.output:
        with args.output.open("w") as f:
            f.write("// Auto-generated by train_mlp_key.py\n")
            f.write(f"// Train accuracy: {train_acc:.1%}\n")
            f.write(f"// CV accuracy: {cv['mean_accuracy']:.1%}\n\n")
            f.write(f"pub const HIDDEN_SIZE: usize = 48;\n")
            f.write(f"pub const NUM_CLASSES: usize = 24;\n\n")
            f.write(format_rust_weights("W1", final_model.w1) + "\n\n")
            f.write(format_rust_weights("B1", final_model.b1.reshape(1, -1)) + "\n\n")
            f.write(format_rust_weights("W2", final_model.w2) + "\n\n")
            f.write(format_rust_weights("B2", final_model.b2.reshape(1, -1)) + "\n\n")
        print(f"\nWeights written to {args.output}")
        total_params = 12 * 48 + 48 + 48 * 24 + 24
        print(f"Total parameters: {total_params} ({total_params * 4} bytes)")
    else:
        total_params = 12 * 48 + 48 + 48 * 24 + 24
        print(f"\nModel size: {total_params} parameters ({total_params * 4 / 1024:.1f} KB)")
        print("Pass --output path/to/weights.rs to export Rust weights.")


if __name__ == "__main__":
    main()
