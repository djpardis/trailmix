"""Write reference logits for the key-lime reranker test from an ONNX export.

The inputs are deterministic pseudo-random chroma frames and DSP scores built
with the same linear congruential generator as the Rust test, so the fixture
stores only the case definitions and the expected 24 logits per case.

Usage:
  uv run --no-project --python 3.13 --with numpy --with onnxruntime \\
    python scripts/key_reranker_reference.py \\
    --onnx labels-only-seed1729.onnx \\
    --output crates/key-lime/tests/fixtures/key-reranker-reference.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import onnxruntime

MAX_FRAMES = 256
CASES = [
    {"seed": 1, "frames": 256},
    {"seed": 2, "frames": 97},
    {"seed": 3, "frames": 400},
    {"seed": 4, "frames": 1},
]


class Lcg:
    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFFFFFF

    def next(self) -> np.float32:
        self.state = (self.state * 1664525 + 1013904223) & 0xFFFFFFFF
        return np.float32(self.state >> 8) / np.float32(16777216.0)


def case_inputs(seed: int, frame_count: int) -> tuple[np.ndarray, np.ndarray]:
    generator = Lcg(seed)
    frames = np.zeros((1, MAX_FRAMES, 12), dtype=np.float32)
    for index in range(frame_count):
        raw = [generator.next() for _ in range(12)]
        if index >= MAX_FRAMES:
            continue
        total = np.float32(0.0)
        for value in raw:
            total = np.float32(total + abs(value))
        frames[0, index] = [value / total for value in raw]
    prior = np.array(
        [[np.float32(2.0) * generator.next() - np.float32(1.0) for _ in range(24)]],
        dtype=np.float32,
    )
    return frames, prior


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--onnx", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()

    session = onnxruntime.InferenceSession(
        arguments.onnx.as_posix(), providers=["CPUExecutionProvider"]
    )
    cases = []
    for case in CASES:
        frames, prior = case_inputs(case["seed"], case["frames"])
        (logits,) = session.run(None, {"chroma_frames": frames, "dsp_prior": prior})
        cases.append({**case, "logits": [float(value) for value in logits[0]]})

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps({"cases": cases}, indent=2) + "\n")


if __name__ == "__main__":
    main()
