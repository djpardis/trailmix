# Trail Mix

[![CI](https://github.com/djpardis/trailmix/actions/workflows/ci.yml/badge.svg)](https://github.com/djpardis/trailmix/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

Trail Mix is an offline audio-analysis toolkit written in Rust. It takes
normalized mono PCM and returns tempo, musical key, beat positions, and compact
waveform data. It runs locally without a network service, GPU, or model
download.

## Structure

- **Beat Salad** estimates global BPM, beat positions with downbeat inference,
  and local tempo segments. Uses autocorrelation with harmonic scoring and a
  gentle octave prior.
- **Key Lime** (v6) estimates global and local major or minor keys from chroma
  features using Goertzel filters, harmonic summation, multiple key profiles
  (Krumhansl-Kessler, Temperley, EDMA), median aggregation, and
  confidence-gated segmentation.
- **Sampler Platter** generates compact min/max/RMS waveform columns.

The `trailmix` crate combines the three analyzers behind one PCM-in/results-out
API. File decoding is kept in the optional `trailmix-codecs` crate.

Supporting crates:

- `trailmix-cli`: command-line analysis of individual files.
- `trailmix-bench`: machine-readable benchmark reports (MIREX-weighted key
  score, beat F1, Serato agreement, timing).
- `trailmix-manifest`: shared corpus annotation schema.
- `trailmix-datasets`: imports GiantSteps reference annotations.
- `test-kitchen`: loopback browser UI for playback, annotation, and on-demand
  analysis.

## Quick start

Trail Mix requires Rust 1.85 or newer. From a clone of this repository:

```sh
cargo run -p trailmix-cli -- path/to/audio.flac
```

The CLI supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 and prints
versioned analysis results as JSON. CI tests Linux, macOS, and Windows.

## Design principles

- **Pure Rust, zero ML dependencies**: all analysis is deterministic DSP.
  Same input always produces the same output. No GPU, no model files, no
  Python runtime.
- **Small binary**: all three analyzers fit in under 3 MB.
- **Portable**: runs on ARM, x86, and WASM without platform-specific
  inference machinery.
- **Interpretable**: intermediate features (chroma vectors, onset envelopes,
  confidence scores) are exposed, not hidden in a black box.
- **Versioned results**: every output carries an algorithm version. Changes
  that alter results increment the version so callers can reanalyze
  selectively.

## Accuracy context

Trail Mix uses heuristic DSP, not trained models. On GiantSteps (EDM):

- Key exact accuracy: ~34% (CNN SOTA: ~73%, transformer SOTA: ~78%)
- Tempo Acc2 (octave-aware): ~90%+
- Beat F1: ~0.35 (TCN SOTA: ~0.85+)

The tradeoff is explicit: lower accuracy in exchange for zero deployment
complexity and full transparency. See [ARCHITECTURE.md](ARCHITECTURE.md) and
[SPEAKER_NOTES.md](SPEAKER_NOTES.md) for detailed algorithm descriptions and
SOTA comparisons.

## License

Trail Mix is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related
notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
