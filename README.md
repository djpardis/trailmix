<p align="center">
  <img src="brand/illustrations/trail-mix.png" alt="An illustrated mix of nuts, seeds, raisins, and chocolate" width="144">
</p>

# trail mix

[![CI](https://github.com/djpardis/trailmix/actions/workflows/ci.yml/badge.svg)](https://github.com/djpardis/trailmix/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)

**trail mix** is an audio-analysis library. It turns audio into versioned
JSON for tempo, beats, key, key segments, and waveform display data.

It is built for local analysis in music applications, library import jobs,
playback prep, annotation tools, and MIR research workflows.
[Cueport](https://usecueport.com/) uses trail mix for local waveform and
audio-analysis features.

## Outputs

A trail mix analysis can include:

- **beat salad**: tempo and beat data.
  `global_bpm` is the precise track-level tempo estimate. `display_bpm` is the
  UI-friendly rounded BPM chosen to fit the beat timing. `tempo_segments` are
  sections where BPM changes over time. `beats` are timed beat positions with a
  confidence value and 4/4 `position_in_bar`.
- **key lime**: global key, local key segments, the key estimator used, and the
  DSP key used as the transformer reranker prior. `key` is the selected global
  key. `segments` are local key regions. `estimator` records whether the result
  came from DSP or the reranker. `dsp_key` keeps the DSP prior. `chroma` stores
  the global pitch-class profile.
- **sampler platter**: compact waveform columns for drawing an overview without
  storing full PCM. `columns` are display buckets. Each column stores `min`,
  `max`, and `rms` values.
- **benchmark features**: optional training and evaluation features.
  `key_chroma_frames` are frame-level chroma vectors. `key_scores` are DSP key
  candidate scores. Confidence values are carried on beat, tempo-segment, key,
  and key-segment outputs.

The output schema is versioned so applications can store results and compare
them across releases.

## Analyzers

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/beat-salad-card.png" alt="beat salad illustration">
  <h3 class="part-name">beat salad</h3>
  <p>Finds the tempo of a track and where its beats fall, including places where the tempo changes.</p>
  <dl class="analyzer-facts">
    <div>
      <dt>crate</dt>
      <dd><span class="part-name">beat-salad</span></dd>
    </div>
  </dl>
  <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/beat-salad">
    <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    <span>View crate</span>
  </a>
</article>

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/key-lime-card.png" alt="key lime illustration">
  <h3 class="part-name">key lime</h3>
  <p>Estimates the musical key of a track and of sections where the key changes. A compact transformer reranker chooses the global key.</p>
  <dl class="analyzer-facts">
    <div>
      <dt>crate</dt>
      <dd><span class="part-name">key-lime</span></dd>
    </div>
  </dl>
  <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/key-lime">
    <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    <span>View crate</span>
  </a>
</article>

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/sampler-platter-card.png" alt="sampler platter illustration">
  <h3 class="part-name">sampler platter</h3>
  <p>Summarizes a track into waveform columns for drawing an overview without storing the full audio.</p>
  <dl class="analyzer-facts">
    <div>
      <dt>crate</dt>
      <dd><span class="part-name">sampler-platter</span></dd>
    </div>
  </dl>
  <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/sampler-platter">
    <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    <span>View crate</span>
  </a>
</article>

## How it works

trail mix has one input shape: mono PCM plus analysis configuration.
`trailmix-codecs` can create that buffer from an audio file when an application
wants trail mix to handle decoding.

Use `trailmix::analyze()` when you want one `analysis-v1` result with tempo,
key, and waveform data. Call beat salad, key lime, or sampler platter directly
when your application only needs one output.

The [Architecture](ARCHITECTURE.md) page explains the internal flow and
algorithms. The [Transformer](TRANSFORMER.md) page explains the key lime
reranker and validation.

## How to use it

Run `trailmix-cli` from the repository root:

```sh
cargo run -p trailmix-cli -- song.mp3
```

Run `trailmix-cli` with key lime's DSP-only estimator by disabling default
features:

```sh
cargo run -p trailmix-cli --no-default-features -- song.mp3
```

Use `trailmix-codecs` through `trailmix_codecs::analyze_path()` when trail mix
should open and decode the file:

```rust
let analysis = trailmix_codecs::analyze_path(
    "song.mp3",
    trailmix::AnalysisConfig::default(),
)?;
```

Use the `trailmix` facade through `trailmix::analyze()` when your application
already has mono PCM:

```rust
let analysis = trailmix::analyze(audio, trailmix::AnalysisConfig::default());
```

## Backend choices

The key lime production default is DSP plus the compact key reranker. The reranker
is embedded in the crate and does not require ONNX Runtime. A DSP-only build is
available with `--no-default-features`.

Future learned or platform-native analyzers should keep the same `analysis-v1`
contract or introduce a new schema version. [BACKENDS.md](BACKENDS.md) lists the
evidence required before a backend changes the production claim.

## Evaluation

The beat salad tempo and beat-grid outputs and sampler platter waveform outputs
are the most mature parts of the implementation. The key lime reranker improves
global key accuracy while keeping the same local result contract. The
[Transformer](TRANSFORMER.md) page summarizes the model and validation.

## FOSSY 2026 talk

The FOSSY 2026 talk,
[trail mix: Open tempo, key, and waveform analysis](https://djpardis.com/talks/trail-mix-fossy-2026/),
explains the original DSP analyzer design. It describes the DSP-only key lime
version of trail mix.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local checks, data rules, and commit
style, and [RELEASE.md](RELEASE.md) for the release process.

## License

**trail mix** is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related notices are
recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
