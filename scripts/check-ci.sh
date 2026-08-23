#!/usr/bin/env bash
# Run the same Rust gates as GitHub Actions before pushing trail mix changes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

echo "Checking formatting..."
cargo fmt --all -- --check

echo "Running clippy with CI settings..."
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo "Running workspace tests with all features..."
cargo test --workspace --all-features --locked
