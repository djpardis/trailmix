# Analyzer backends

trail mix supports two development tracks in the same repository.

## DSP default

The default crates use deterministic Rust DSP:

- `beat-salad` for tempo, beat positions, and local tempo segments.
- `key-lime` for global and local key estimation.
- `sampler-platter` for compact waveform summaries.
- `trailmix` for the combined `analysis-v1` result.
- `trailmix-codecs` for optional audio-file decoding.

This path is the default for Cueport and other applications that want local
analysis without model checkpoints or an inference runtime. It builds without
the benchmark model dependencies and keeps the `analysis-v1` JSON contract as
the production integration target.

## Transformer and model candidates

First-party transformer or foundation-model work lives in `trailmix-models`.
That crate defines the backend contract and metadata required for learned
analysis while keeping the default DSP crates independent from model runtimes
and weights.

External projects remain benchmark baselines. They run through the external
baseline harness under `benchmarks/baselines`, where source revisions, artifact
hashes, licenses, training disclosures, preprocessing, and output conversion
are recorded for each tool. The main Rust crates do not ship external model
weights, conversion scripts, or third-party model execution code.

Concrete first-party model implementations can be added to `trailmix-models`
after review. Cueport should receive a learned backend only when the
application explicitly chooses the model artifact and runtime.

## Comparison requirements

Every model backend report must include:

- Same-corpus accuracy under the same metric definitions as the DSP analyzer.
- Mean and median analysis time per track.
- Decode time separated from inference time when possible.
- Wall-clock throughput on the same machine and thread count.
- Model checkpoint size.
- Runtime dependency size and required cache files.
- Output JSON size under `analysis-v1`.
- Hardware support for CPU, Apple Silicon, GPU, and WebAssembly.
- Code license, weight license, and disclosed training datasets.

Accuracy improvements should be reported together with their runtime,
packaging, hardware, and licensing costs. The default DSP path remains the
reference implementation for applications while the first-party model track
develops in parallel.
