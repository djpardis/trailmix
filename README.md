# Trail Mix

[![CI](https://github.com/djpardis/trailmix/actions/workflows/ci.yml/badge.svg)](https://github.com/djpardis/trailmix/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

Trail Mix is an experimental, offline audio-analysis toolkit written in Rust.
It takes normalized mono PCM and returns tempo, musical-key, and compact
waveform data. It runs locally without a network service or model download.

## Structure

- **Beat Salad** estimates global BPM, beat positions, and local tempo segments.
- **Key Lime** estimates global and local major or minor keys from chroma
  features.
- **Sampler Platter** generates compact min/max/RMS waveform columns.

The `trailmix` crate combines the three analyzers behind one PCM-in/results-out
API. File decoding is kept in the optional `trailmix-codecs` crate, and
`trailmix-cli` provides a command-line interface for analyzing individual
files.

## Quick start

Trail Mix requires Rust 1.85 or newer. From a clone of this repository:

```sh
cargo run -p trailmix-cli -- path/to/audio.flac
```

The CLI supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 and prints
versioned analysis results as JSON. CI tests Linux, macOS, and Windows.

## License

Trail Mix is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related
notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
