<p align="center">
  <img src="brand/illustrations/trail-mix.png" alt="An illustrated mix of nuts, seeds, raisins, and chocolate" width="144">
</p>

# trail mix

[![CI](https://github.com/djpardis/trailmix/actions/workflows/ci.yml/badge.svg)](https://github.com/djpardis/trailmix/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

**trail mix** is an audio-analysis toolkit written in Rust. It accepts an
audio file or mono PCM and returns tempo, musical key, beat positions, and
compact waveform data as JSON.

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

## Structure

- **beat salad** estimates global BPM, beat positions with downbeat inference,
  and local tempo segments. Uses autocorrelation with harmonic scoring and a
  gentle octave prior.
- **key lime** estimates global and local major or minor keys using dual chroma
  extraction (Goertzel + Constant-Q Transform), multiple key profiles
  (Krumhansl-Kessler, Temperley, EDMA, and learned), median aggregation, and
  confidence-gated segmentation.
- **sampler platter** generates compact min/max/RMS waveform columns.

The `trailmix` crate combines the three analyzers behind one PCM-in/results-out
API. The optional `trailmix-codecs` crate opens audio files, decodes them, preps
mono PCM, and runs the same analysis.

The workspace also includes `trailmix-cli` for command-line use,
`trailmix-bench` for machine-readable benchmarks, `trailmix-manifest` for the
corpus annotation schema, `trailmix-datasets` for importing GiantSteps
reference annotations, and `test-kitchen` as a loopback browser UI for
playback, annotation, and on-demand analysis.

## Design principles

- **Pure Rust**: all analysis is deterministic DSP.
- **Small binary**: all three analyzers fit in under 3 MB.
- **Portable**: runs on ARM, x86, and WASM.
- **Interpretable**: intermediate features (chroma vectors, onset envelopes,
  confidence scores) are exposed.

## Accuracy and next step

**trail mix** uses lightweight DSP. It runs locally, remains small, and
works across targets. BPM, beat, and waveform analysis are useful now. Key
estimation needs the most improvement.

The next step is optional model-based or platform-native analysis that returns
the same `Analysis` JSON. The DSP core remains the default for apps that need
local, small, dependency-light analysis.

## License

**trail mix** is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related
notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
