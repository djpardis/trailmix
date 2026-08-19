<p align="center">
  <img src="brand/illustrations/trail-mix.png" alt="An illustrated mix of nuts, seeds, raisins, and chocolate" width="144">
</p>

# trail mix

[![CI](https://github.com/djpardis/trailmix/actions/workflows/ci.yml/badge.svg)](https://github.com/djpardis/trailmix/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

**trail mix** is an audio-analysis toolkit written in Rust. It produces tempo,
musical key, beat positions, and compact waveform data as JSON.

[Cueport](https://usecueport.com/) uses **trail mix** for local waveform
and audio-analysis features, but the crates are built for any desktop, mobile,
server, or research tool that needs local tempo, key, and waveform analysis.

## Quick start

**trail mix** requires Rust 1.85 or newer. Run the CLI from the repository root.

```sh
cargo run -p trailmix-cli -- song.mp3
```

The CLI supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 and prints
versioned analysis results as JSON. CI tests Linux, macOS, and Windows.

`trailmix_codecs::analyze_path()` accepts an audio file and handles decoding,
mono PCM prep, and analysis.

```rust
let analysis = trailmix_codecs::analyze_path(
    "song.mp3",
    trailmix::AnalysisConfig::default(),
)?;
let json = serde_json::to_string(&analysis)?;
```

When the application already has mono PCM, call the core API directly.

```rust
let analysis = trailmix::analyze(audio, trailmix::AnalysisConfig::default());
```

## Benchmarking against a Cueport Serato folder

`trailmix-bench` can compare trail mix output against the Serato BPM and key
values that Cueport imported for a specific folder. The command prints progress
for each file, records the trail mix git SHA, hashes the exact audio file, and
writes optional JSON Lines output as each track finishes.

```sh
cargo run -p trailmix-bench --release --features cueport-db -- \
  --cueport-db "$HOME/Library/Application Support/com.cueport.app/cueport.db" \
  --cueport-serato-folder "$HOME/Music/Music/Download library/mp3s/qobuz/qobuz-2026-01" \
  --jsonl /tmp/trailmix-qobuz.jsonl
```

The summary separates decoder failures, tracks with no detected BPM, ordinary
BPM error, octave-aware BPM error, MIREX Accuracy 1 and Accuracy 2, tempo
ratio buckets, exact key agreement, MIREX key score, and key-relationship
buckets.

## Structure

- **beat salad** estimates global BPM, beat positions with downbeat inference,
  and local tempo segments. Uses autocorrelation with harmonic scoring and a
  gentle octave prior.
- **key lime** estimates global and local major or minor keys using dual chroma
  extraction (Goertzel + Constant-Q Transform), averaged scores from multiple
  key profiles (Krumhansl-Kessler, Temperley, EDMA, and learned), median
  aggregation, and confidence-gated segmentation.
- **sampler platter** generates compact waveform columns with raw min/max/RMS
  values plus optional display height and band-energy color hints.

The `trailmix` crate combines the three analyzers behind one results API. The
optional `trailmix-codecs` crate opens audio files and runs the same analysis.

The workspace also includes `trailmix-cli` for command-line use,
`trailmix-bench` for machine-readable benchmarks, `trailmix-manifest` for the
corpus annotation schema, `trailmix-datasets` for importing GiantSteps
reference annotations, and `test-kitchen` as a loopback browser UI for
playback, annotation, and on-demand analysis.

## Design principles

- **Rust**: all analysis is deterministic DSP.
- **Small binary**: all three analyzers fit in < 3 MB.
- **Portable**: runs on ARM, x86, and WASM.
- **Interpretable**: intermediate features available for inspection (chroma
  vectors, onset envelopes, confidence scores).

## Accuracy and next step

BPM, beat-grid, and waveform outputs are the most mature parts of the current
implementation. Key estimation has the largest accuracy gap and is the main
target for improvement.

The next step is optional model-based or platform-native analysis. The DSP core
remains the default for apps that need local, dependency-light analysis.

## License

**trail mix** is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related
notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
