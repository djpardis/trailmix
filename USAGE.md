# Usage

trail mix accepts mono PCM plus analysis configuration. Use the
<strong class="crate-name">trailmix</strong> facade for one versioned result,
or call an analyzer crate when an application only needs tempo, key, or waveform
data.

## Command line

Run <strong class="crate-name">trailmix-cli</strong> from the repository root:

```sh
cargo run -p trailmix-cli -- song.mp3
```

Run the DSP-only key estimator by disabling default features:

```sh
cargo run -p trailmix-cli --no-default-features -- song.mp3
```

## Decode an audio file

Use <strong class="crate-name">trailmix-codecs</strong> when trail mix should
open and decode the audio file:

```rust
let analysis = trailmix_codecs::analyze_path(
    "song.mp3",
    trailmix::AnalysisConfig::default(),
)?;
```

## Analyze PCM

Use the <strong class="crate-name">trailmix</strong> facade when an application
already has mono PCM:

```rust
let analysis = trailmix::analyze(audio, trailmix::AnalysisConfig::default());
```

## Choose one analyzer

Call <strong class="crate-name">beat-salad</strong> for tempo and beats,
<strong class="crate-name">key-lime</strong> for key analysis, or
<strong class="crate-name">sampler-platter</strong> for waveform columns.
See [Architecture](ARCHITECTURE.md) for the input and output details.
