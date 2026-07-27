# Trail Mix

Trail Mix is an experimental Rust toolkit for offline audio analysis. It accepts
normalized mono PCM and returns tempo, musical-key, and compact waveform data
without a network service or model download.

Trail Mix is a research prototype. Its synthetic tests check basic behavior,
but its accuracy and confidence values have not yet been validated on a
representative music corpus.

## Quick start

The workspace requires Rust 1.85 or newer. From a clone of this repository,
analyze a supported audio file with:

```sh
cargo run -p trailmix-cli -- path/to/audio.flac
```

The CLI supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4. It prints
versioned JSON containing global and local tempo estimates, beat positions,
global and local key estimates, chroma, confidence values, and waveform
columns.

To check the toolkit without supplying an audio file, run its deterministic
synthetic smoke benchmark:

```sh
cargo run --release -p trailmix-bench
```

The synthetic benchmark catches basic regressions. It does not measure accuracy
on recorded music.

## Use the library

The `trailmix` crate provides one PCM-in/results-out API:

```rust
let analysis = trailmix::analyze(
    trailmix::AudioBuffer {
        samples: &mono_pcm,
        sample_rate: 44_100,
    },
    trailmix::AnalysisConfig::default(),
);
```

The core analysis crates do not decode files, so applications that already have
PCM do not take on codec dependencies. The optional `trailmix-codecs` crate
provides feature-gated file decoding.

## Analysis components

- **Beat Salad** estimates global BPM, beat positions, and local tempo segments.
- **Key Lime** estimates global and local major or minor keys from chroma
  features.
- **Sampler Platter** generates compact min/max/RMS waveform columns.

The `trailmix` facade crate runs these three analyzers and returns their
versioned results together. Each analyzer can also be used independently. See
[ARCHITECTURE.md](ARCHITECTURE.md) for the complete crate map, processing
boundary, result-versioning policy, and current limitations.

## Evaluation

Real-track evaluation uses private, ignored manifests and audio files. The
repository also includes tools for importing public annotations and reviewing
them in the local Test Kitchen interface. See
[BENCHMARKING.md](BENCHMARKING.md) for setup, supported metrics, dataset
references, and data-handling rules.

No accuracy claim should be based on the synthetic benchmark alone. Planned
evaluation work and time-varying analysis milestones are tracked in
[ROADMAP.md](ROADMAP.md).

## Documentation

- [Architecture](ARCHITECTURE.md)
- [Benchmarking and annotation](BENCHMARKING.md)
- [Contributing](CONTRIBUTING.md)
- [Roadmap](ROADMAP.md)
- [Third-party software](THIRD_PARTY.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks, public-data
requirements, and the documentation checklist used to keep these files aligned
with the code.

## License

Trail Mix is available under either the [MIT License](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option. Codec-related
notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
