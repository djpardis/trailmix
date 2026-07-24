# Architecture

## Boundary

Trail Mix accepts finite mono `f32` PCM plus a sample rate. Decoding, file
metadata, databases, application state, and network transport remain caller
responsibilities. This boundary keeps the analysis crates usable in desktop,
command-line, and research tools without coupling them to one application.

## Crates

- `trailmix` is the facade and versioned aggregate result.
- `beat-salad` calculates an energy-onset envelope, autocorrelation tempo
  candidates, a beat grid, and windowed tempo segments.
- `key-lime` builds a pitch-class profile with windowed Goertzel measurements
  and compares it with rotated major and minor key profiles.
- `sampler-platter` bins PCM into min, max, and RMS waveform columns.
- `trailmix-codecs` provides separately selectable common-format decoders and
  downmixes decoded channels to mono PCM.
- `trailmix-cli` analyzes one supported audio file for manual evaluation.
- `trailmix-bench` emits machine-readable synthetic or manifest-driven
  real-track benchmark results.

## Result stability

Every public result includes a version. Algorithm changes that alter result
meaning must increment the relevant component version. Applications should
store that version alongside generated values and reanalyze only when their
own policy requires it.

## Current limitations

- Tempo estimation uses a compact energy-flux baseline, not a trained model.
- Tempo segments represent locally stable estimates and do not yet model a
  continuous ramp between two BPM values.
- Key estimation does not yet estimate tuning offset or key changes.
- The decoder supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 when
  the corresponding Cargo features are enabled.
- Confidence values are preliminary and have not been calibrated on held-out
  recordings.
