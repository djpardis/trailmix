"""Export a key reranker checkpoint to the flat weight file read by key-lime.

The input is the JSON artifact written by the key-lime transformer trainer. The
output is a headerless little-endian float32 file whose tensor order and shapes
must match `crates/key-lime/src/reranker.rs`. The script refuses checkpoints
whose architecture differs from the one the Rust forward pass implements.

Usage:
  uv run --no-project --python 3.13 python scripts/export_key_reranker_weights.py \\
    --checkpoint labels_only-seed1729.json \\
    --output crates/key-lime/models/key-reranker-v1.f32
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

ARCHITECTURE = {
    "type": "compact_transformer_encoder",
    "d_model": 36,
    "heads": 4,
    "ffn_dim": 72,
    "layers": 1,
    "max_frames": 256,
    "feature_dim": 12,
}

TENSORS = [
    ("input_projection.conv.weight", (3, 1, 3)),
    ("input_projection.conv.bias", (3,)),
    ("position", (1, 256, 36)),
    ("encoder.layers.0.norm1.weight", (36,)),
    ("encoder.layers.0.norm1.bias", (36,)),
    ("encoder.layers.0.self_attn.in_proj_weight", (108, 36)),
    ("encoder.layers.0.self_attn.in_proj_bias", (108,)),
    ("encoder.layers.0.self_attn.out_proj.weight", (36, 36)),
    ("encoder.layers.0.self_attn.out_proj.bias", (36,)),
    ("encoder.layers.0.norm2.weight", (36,)),
    ("encoder.layers.0.norm2.bias", (36,)),
    ("encoder.layers.0.linear1.weight", (72, 36)),
    ("encoder.layers.0.linear1.bias", (72,)),
    ("encoder.layers.0.linear2.weight", (36, 72)),
    ("encoder.layers.0.linear2.bias", (36,)),
    ("norm.weight", (36,)),
    ("norm.bias", (36,)),
    ("output.weight", (24, 36)),
    ("output.bias", (24,)),
    ("prior_projection.weight", (24, 24)),
]


def flatten(value: object, shape: tuple[int, ...], name: str) -> list[float]:
    if not shape:
        return [float(value)]
    if not isinstance(value, list) or len(value) != shape[0]:
        raise SystemExit(f"{name}: expected leading dimension {shape[0]}")
    flat: list[float] = []
    for item in value:
        flat.extend(flatten(item, shape[1:], name))
    return flat


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()

    checkpoint = json.loads(arguments.checkpoint.read_text())
    architecture = checkpoint.get("architecture", {})
    for key, expected in ARCHITECTURE.items():
        if architecture.get(key) != expected:
            raise SystemExit(f"architecture.{key} is {architecture.get(key)!r}, expected {expected!r}")
    training = checkpoint.get("training", {})
    if not training.get("pitch_equivariant_projection") or not training.get("dsp_prior"):
        raise SystemExit("checkpoint must use the circular input stage and the DSP prior")
    if training.get("pitch_conv_kernel_size") != 3:
        raise SystemExit("checkpoint must use a three-bin circular kernel")

    state = checkpoint["state_dict"]
    unexpected = sorted(set(state) - {name for name, _ in TENSORS})
    if unexpected:
        raise SystemExit(f"unexpected tensors: {', '.join(unexpected)}")

    values: list[float] = []
    for name, shape in TENSORS:
        values.extend(flatten(state[name], shape, name))

    payload = struct.pack(f"<{len(values)}f", *values)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_bytes(payload)
    print(
        json.dumps(
            {
                "parameters": len(values),
                "bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
                "checkpoint_sha256": hashlib.sha256(arguments.checkpoint.read_bytes()).hexdigest(),
                "seed": training.get("seed"),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
