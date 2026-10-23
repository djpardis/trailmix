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

## Overview

trail mix is a local audio-analysis library. It has three independent analyzers
for tempo and beats, key, and waveform. All three accept decoded mono PCM. The
combined API runs them together and returns an aggregate result.
[Usage](USAGE.md) documents these interfaces, and
[Architecture](ARCHITECTURE.md) describes their execution and result types.

Tempo and beat tracking use deterministic DSP. A 24-band spectral-flux onset
function feeds autocorrelation-based tempo estimation and dynamic-programming
beat placement. Across the GiantSteps full-track corpus and the FSL10K
short-loop corpus, global BPM falls within 4 percent of the reference for about
40 to 70 percent of files. Accepting half- and double-time equivalents raises
the range to about 60 to 80 percent.

Waveform analysis groups the PCM into display columns and stores the minimum,
maximum, and RMS energy for each column. These values retain the information
needed to draw a detailed overview without storing the full PCM.

Key estimation combines DSP with a compact learned component. DSP computes
tuned twelve-bin pitch-class profiles, scores the 24 major and minor keys, and
identifies local key segments. For global key, profile scoring reached a MIREX
key score of about 61 on a sealed holdout. A
[compact transformer reranker](TRANSFORMER.md) using the same pitch-class
sequence and DSP scores raises the MIREX key score to about 74, close to larger
learned baselines on the same corpus. The transformer is used only for global
key selection, and its embedded Rust implementation requires no external
model runtime.

## FOSSY 2026 talk

The FOSSY 2026 talk,
[trail mix: Open tempo, key, and waveform analysis](https://djpardis.com/talks/trail-mix-fossy-2026/),
introduces the DSP analysis for tempo, beat, key, and waveform data. It predates
the compact key reranker, so its key results use the DSP-only estimator.

## Analyzers

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/beat-salad-card.png" alt="beat salad illustration">
  <div class="analyzer-title-row">
    <h3 class="part-name">beat salad</h3>
    <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/beat-salad" aria-label="View beat-salad crate">
      <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    </a>
  </div>
  <p>Finds the tempo of a track and where its beats fall, including places where the tempo changes.</p>
</article>

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/key-lime-card.png" alt="key lime illustration">
  <div class="analyzer-title-row">
    <h3 class="part-name">key lime</h3>
    <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/key-lime" aria-label="View key-lime crate">
      <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    </a>
  </div>
  <p>Estimates the musical key of a track and of sections where the key changes. A compact transformer reranker chooses the global key.</p>
</article>

<article class="analyzer-card">
  <img class="analyzer-logo" src="docs/sampler-platter-card.png" alt="sampler platter illustration">
  <div class="analyzer-title-row">
    <h3 class="part-name">sampler platter</h3>
    <a class="crate-link" href="https://github.com/djpardis/trailmix/tree/main/crates/sampler-platter" aria-label="View sampler-platter crate">
      <svg class="crate-link-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 17 17 7"/><path d="M8 7h9v9"/></svg>
    </a>
  </div>
  <p>Summarizes a track into waveform columns for drawing an overview without storing the full audio.</p>
</article>

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local checks, data rules, and commit
style, and [RELEASE.md](RELEASE.md) for the release process.

## License

**trail mix** is available under your choice of the [MIT License](LICENSE-MIT) or
the [Apache License, Version 2.0](LICENSE-APACHE). Codec-related notices are
recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
