# Trail Mix

Trail Mix is an experimental, offline audio-analysis toolkit written in Rust. It
accepts normalized mono PCM and produces tempo, musical-key, and compact
waveform data without a network service or model download.

The current implementation is a research prototype. Its synthetic tests verify
basic behavior, but its accuracy has not yet been established on a representative
music corpus.

## Components

- **Beat Salad** estimates global BPM, beat positions, and local tempo segments.
- **Key Lime** estimates a global major or minor key from chroma features.
- **Sampler Platter** generates compact min/max/RMS waveform columns.
- **Trail Mix** provides one PCM-in/results-out facade over all three analyzers.

All components live in one Cargo workspace so they can share releases and test
data while remaining independently usable.

## Use the library

```rust
let analysis = trailmix::analyze(
    trailmix::AudioBuffer {
        samples: &mono_pcm,
        sample_rate: 44_100,
    },
    trailmix::AnalysisConfig::default(),
);
```

The core crates do not decode files. This keeps codec dependencies out of
applications that already have PCM. The optional `trailmix-codecs` crate
provides feature-gated MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4
decoding. The included CLI enables those common formats:

```sh
cargo run -p trailmix-cli -- path/to/audio.flac
```

It prints versioned JSON containing BPM, confidence, beat positions, tempo
segments, key, chroma, and waveform columns.

Run the deterministic synthetic smoke benchmark with:

```sh
cargo run --release -p trailmix-bench
```

This benchmark catches basic regressions. It is not evidence of accuracy on
recorded music.

For private real-track evaluation, copy `benchmarks/manifest.example.json`,
place audio under the ignored `benchmarks/local/` directory, add your
annotations, and run:

```sh
cargo run --release -p trailmix-bench -- \
  --manifest benchmarks/manifest.private.json
```

The report contains per-track and aggregate global BPM error, octave-aware BPM
error, exact key accuracy, local tempo-segment error, decoding time, and
analysis time. Reports include track IDs but not source paths. Do not commit
private manifests, recordings, or generated reports.

## Development

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Do not commit commercial recordings or private benchmark data. The repository
ignores common audio formats by default. Future public fixtures must have clear
redistribution terms.

## Research direction

The next milestone is a reproducible benchmark covering:

- exact and octave-aware global BPM accuracy
- piecewise and gradually changing tempo
- half-time and double-time ambiguity
- global key accuracy and musically related errors
- confidence calibration
- runtime, memory use, and binary size

Published accuracy claims should be based on held-out, legally usable data and
immutable source revisions.

## License

Trail Mix is available under either the MIT License or the Apache License,
Version 2.0, at your option.
