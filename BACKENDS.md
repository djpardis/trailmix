# Analyzer backends

trail mix supports a production default, a DSP-only build, and a contract for
future learned or platform-native analyzers.

## Production default

The default `trailmix` crate runs deterministic Rust DSP for tempo, beat,
waveform, local key segments, chroma extraction, and 24-key DSP scores. Global
key estimation then uses the compact key lime reranker when the `reranker`
feature is enabled. The reranker embeds the selected seed-1729 weights and runs
a pure Rust forward pass, so applications do not need ONNX Runtime, a model
server, or a native inference library.

The combined result remains serialized `Analysis` with the same top-level
schema version unless a backend needs an incompatible shape. `KeyAnalysis`
records the estimator used for the global key and keeps the DSP key available
for comparison.

## DSP-only build

Applications that need deterministic DSP only can disable default features:

```sh
cargo build -p trailmix --no-default-features
cargo run -p trailmix-cli --no-default-features -- song.mp3
```

This build keeps beat, tempo, waveform, chroma, local key segments, and DSP key
estimation. It omits the embedded reranker weights and reports `dsp` as the key
estimator.

## Codec wrapper

`trailmix` starts from mono PCM supplied by the application. `trailmix-codecs`
opens audio files, decodes common formats through Symphonia, prepares mono PCM,
and calls the same analyzer. The codec wrapper is optional so applications that
already own decoding do not take codec dependencies.

## Future learned and platform-native analyzers

`trailmix-models` defines the metadata and result contract for additional
learned analyzers. Future backends should preserve the shipped serialized
`Analysis` shape or introduce a new top-level schema version when the meaning of
existing fields changes.

External projects remain benchmark baselines under `benchmarks/baselines`,
where source revisions, artifact hashes, licenses, training disclosures,
preprocessing, and output conversion are recorded before a result is used.

## Comparison requirements

Every backend report must include:

- Same-corpus accuracy under the same metric definitions as the current
  analyzer.
- Mean and median analysis time per track.
- Decode time separated from inference time when possible.
- Wall-clock throughput on the same machine and thread count.
- Model checkpoint size when the backend uses weights.
- Runtime dependency size and required cache files.
- Output JSON size for `Analysis`.
- Hardware support for CPU, Apple Silicon, GPU, and WebAssembly.
- Code license, weight license, and disclosed training datasets.

Accuracy improvements should be reported together with their runtime,
packaging, hardware, and licensing costs. If a backend is chosen because it is
smaller, faster, or easier to ship, report the measured accuracy difference
with the measured efficiency gain and the application reason for that choice.
