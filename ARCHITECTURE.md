# Architecture

## Boundary

Trail Mix accepts finite mono `f32` PCM plus a sample rate. Decoding, file
metadata, databases, application state, and network transport remain caller
responsibilities. This boundary keeps the analysis crates usable in desktop,
command-line, and research tools without coupling them to one application.

## Crates

- `trailmix` is the facade and versioned aggregate result.
- `beat-salad` calculates an energy-onset envelope, autocorrelation tempo
  candidates, a beat grid, and windowed tempo segments. Tempo estimation
  applies a stronger octave prior (25% weight) with sub-harmonic checking to
  reduce half/double errors. `BeatPosition` includes a `position_in_bar`
  field (1-4, assumes 4/4 meter) for downbeat inference.
- `key-lime` (v5) builds frame-level pitch-class profiles with Goertzel
  measurements and classifies key at global and segment levels.
  - Harmonic summation: for each fundamental note, energy from its 2nd, 3rd,
    and 4th harmonics (at +12, +19, +24 semitones) is summed with weights
    0.5, 0.33, and 0.25, preventing harmonics from polluting unrelated pitch
    classes.
  - Multiple key profiles: scores chroma against Krumhansl-Kessler, Temperley,
    and EDMA profiles and picks the best correlation, handling different
    musical styles.
  - Tuning estimation: detects sub-semitone pitch offset via parabolic
    interpolation on chroma peaks and shifts chroma before classification.
  - Onset-weighted chroma: frames with rising energy (attacks) contribute more
    to global chroma than sustained tails.
  - Spectral whitening: power-law compression (gamma=0.5) of chroma bins
    before normalization prevents dominant frequencies from overwhelming the
    vector.
  - Confidence-gated segmentation: `segment_confidence_threshold` (default
    0.15) prevents low-confidence local windows from creating segment
    boundaries; `minimum_segment_seconds` (default 4.0) merges short
    segments.
  - Segment-majority voting: global key uses the longest segment's key instead
    of aggregate chroma classification.
- `sampler-platter` bins PCM into min, max, and RMS waveform columns.
- `trailmix-codecs` provides separately selectable common-format decoders and
  downmixes decoded channels to mono PCM.
- `trailmix-manifest` defines and validates shared private-corpus annotations.
- `trailmix-datasets` imports GiantSteps Tempo v2 and GiantSteps Key reference
  annotations into the shared manifest while retaining dataset provenance.
- `trailmix-cli` analyzes one supported audio file for manual evaluation.
- `trailmix-bench` emits machine-readable synthetic or manifest-driven
  real-track benchmark results.
- `test-kitchen` serves a loopback-only browser interface for playback,
  annotation, Serato observations, and on-demand analysis.

## Result stability

Every public result includes a version. Algorithm changes that alter result
meaning must increment the relevant component version. Applications should
store that version alongside generated values and reanalyze only when their
own policy requires it.

## Current limitations

- Tempo estimation uses a compact energy-flux baseline, not a trained model.
- Tempo segments represent locally stable estimates and do not yet model a
  continuous ramp between two BPM values. When a secondary tempo covers at
  least 25% of the file (configurable), Beat Salad sets `multi_tempo` and
  fills `alternate_bpm` so apps can mark the primary BPM and show the other
  song’s tempo after a beat switch. Key Lime does the same with `multi_key`
  and `alternate_key`. This is for mashups and medleys in one file, not for
  half vs double counting of one pulse.
- Key segmentation does not yet represent modal and no-key sections.
- The decoder supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 when
  the corresponding Cargo features are enabled.
- Confidence values are preliminary and have not been calibrated on held-out
  recordings.
