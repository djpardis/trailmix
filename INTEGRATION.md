# Integration contract

This document defines the Trail Mix contract that applications such as Cueport
can build against while the analysis algorithms continue to improve.

## Stability model

Trail Mix is a library-first API. The stable integration boundary is:

1. Caller provides finite mono `f32` PCM samples and a sample rate.
2. Caller chooses an `AnalysisConfig`.
3. Trail Mix returns an `Analysis` value that implements
   [`serde::Serialize`](https://docs.rs/serde/latest/serde/trait.Serialize.html).
4. Applications may store or transmit the JSON form of that value.

The production contract is the versioned JSON shape, not the current internal
DSP implementation. Algorithm improvements may change estimates and confidence
values without changing the JSON schema. The current wire contract is
`analysis-v1`.

## Input

Call `trailmix::analyze()` with:

- `AudioBuffer.samples`: normalized mono PCM, nominally in `-1.0..=1.0`.
- `AudioBuffer.sample_rate`: source sample rate in hertz.
- `AnalysisConfig`: analyzer configuration.

Decoding, channel downmixing, file metadata, persistence, permissions, sync, and
network transport are caller responsibilities. The optional `trailmix-codecs`
crate is useful for command-line and desktop tools, but apps may decode with
platform APIs. The analysis contract starts after decoding.

`AnalysisConfig::default()` is the recommended first integration target:

- Beat analysis: `BeatConfig::default()`.
- Key analysis: `KeyConfig::default()`.
- Waveform columns: `1500`.

## Output

The aggregate result has this top-level shape:

```json
{
  "version": 1,
  "duration_seconds": 180.0,
  "beat": {},
  "key": {},
  "waveform": {}
}
```

Each nested analyzer also carries its own version. Applications should persist
all component versions, not only the aggregate version:

- `Analysis.version`: `1`
- `BeatAnalysis.version`: `3`
- `KeyAnalysis.version`: `6`
- `WaveformOverview.version`: `1`

The JSON Schema for this contract is stored at
`schemas/analysis-v1.schema.json`.

## Beat result

`beat.global_bpm` is the primary BPM estimate, or `null` when unavailable.
`beat.beats` contains beat positions in seconds. `position_in_bar` is 1-based
and currently assumes 4/4 meter.

`multi_tempo` is for files with a meaningful secondary tempo, such as edits,
medleys, or mashups. It is not a half-time or double-time ambiguity flag.
When `multi_tempo` is true, `alternate_bpm` and `alternate_coverage` describe
the longest secondary tempo region.

## Key result

`key.key` is the primary musical key, or `null` when unavailable. Keys serialize
as structured values:

```json
{
  "tonic": "A",
  "mode": "Minor"
}
```

`tonic` uses the Rust enum spellings `C`, `CSharp`, `D`, `DSharp`, `E`, `F`,
`FSharp`, `G`, `GSharp`, `A`, `ASharp`, and `B`. `mode` is `Major` or `Minor`.

Cueport can convert this structure to display formats such as Camelot, Open Key,
or localized labels in its own UI. If multiple applications need the same
notation helpers, they should be added as explicit Trail Mix helper APIs without
changing the stored key shape.

`multi_key` follows the same meaning as `multi_tempo`: a secondary key covers
enough of the file to matter. It is not a nearby-key scoring tie.

## Waveform result

`waveform.columns` contains compact display columns with `min`, `max`, and
`rms` values. The number of columns is bounded by the requested
`AnalysisConfig.waveform_columns` and the source sample count.

Samples are treated as finite mono PCM. Non-finite values are handled as silence
by waveform generation.

## Compatibility rules

Keep `analysis-v1` stable. Publish `analysis-v2` for incompatible JSON changes.

Applications should treat confidence values as advisory for now. They are useful
for ranking and UI hints, but they are not calibrated enough for hard quality
gates.

## Cueport integration path

The recommended first Cueport integration is a thin Swift/FFI binding that
accepts mono `Float32` PCM plus a sample rate and returns `Analysis` JSON. This
keeps the Swift boundary stable while Trail Mix continues to improve internally.

Cueport should own:

- Audio-file access and decoding.
- Library scanning and persistence.
- UI-specific key notation and formatting.

Trail Mix should own:

- Offline BPM, beat, key, and waveform analysis.
- Versioned result structures.
- Contract tests and schema fixtures.
- Analyzer benchmarks and regression checks.
