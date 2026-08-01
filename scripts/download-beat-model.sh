#!/usr/bin/env bash
# Download the Beat This! ONNX model for beat tracking.
#
# Source: CPJKU/beat_this (ISMIR 2024)
# Paper: "Beat This! Accurate Beat Tracking Without DBN Postprocessing"
#        Foscarin, Schluter, Widmer (2024)
#
# The model accepts log-mel spectrograms [1, T, 128] at 44100 Hz / hop 441
# and outputs beat+downbeat logits [1, T, 2].
#
# Usage: ./scripts/download-beat-model.sh [output_dir]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${1:-$REPO_ROOT/models}"

MODEL_URL="https://cloud.cp.jku.at/index.php/s/7ik4RrBKTS273gp/download?path=%2F&files=final0.pt"
ONNX_CONVERT_REPO="https://github.com/Rliop913/beat_this_onnxconvert"

mkdir -p "$OUTPUT_DIR"

echo "Beat This! ONNX model download"
echo ""
echo "The official model weights are hosted at:"
echo "  https://cloud.cp.jku.at/index.php/s/7ik4RrBKTS273gp"
echo ""
echo "To get the ONNX file, you have two options:"
echo ""
echo "1. Use the pre-converted ONNX from the beat-this Rust crate:"
echo "   curl -L -o $OUTPUT_DIR/beat_this.onnx \\"
echo "     https://github.com/Rliop913/beat_this_onnxconvert/releases/latest/download/beat_this_model_final0.onnx"
echo ""
echo "2. Convert from PyTorch checkpoint yourself:"
echo "   pip install beat-this"
echo "   python -c \"from beat_this.inference import load_model; import torch; m = load_model('final0'); torch.onnx.export(m, ...)\""
echo ""
echo "After downloading, place the .onnx file at:"
echo "  $OUTPUT_DIR/beat_this.onnx"
echo ""
echo "Then build with the onnx-beat feature:"
echo "  cargo build -p beat-salad --features onnx-beat"
echo ""

# Attempt automatic download from the onnxconvert releases
ONNX_URL="https://github.com/b451c/ReaBeat/releases/download/v2.0.0-model/beat_this_final0.onnx"
TARGET="$OUTPUT_DIR/beat_this.onnx"

if command -v curl &>/dev/null; then
    echo "Attempting download from $ONNX_URL ..."
    if curl -fsSL -o "$TARGET" "$ONNX_URL" 2>/dev/null; then
        SIZE=$(wc -c < "$TARGET" | tr -d ' ')
        echo "Downloaded: $TARGET ($SIZE bytes)"
    else
        echo "Automatic download failed. Please download manually using the URLs above."
        exit 1
    fi
else
    echo "curl not found. Please download manually."
    exit 1
fi
