# Trail Mix roadmap

## Product goal

Trail Mix must analyze time-varying music, not only assign one BPM and one key
to an entire track. Its timeline should represent:

- individual beat timestamps
- stable BPM sections
- abrupt BPM changes
- gradual tempo ramps
- stable key sections
- key changes and modulations
- structural beat-switch boundaries
- confidence and algorithm version for every result

A beat switch may change the arrangement without changing BPM or key, or it may
change structure, BPM, and key at the same boundary. Trail Mix must represent
these as separate observations that can occur together.

## Current baseline

- Beat Salad estimates global BPM, beat positions, and preliminary local tempo
  segments.
- Key Lime estimates global key and preliminary windowed key segments.
- Sampler Platter generates compact waveform summaries.
- Trail Mix combines the three analyzers behind a versioned PCM-in/results-out
  API.
- The benchmark harness measures global BPM, octave-aware BPM, key, local
  tempo-segment, decoding, and runtime results.
- The dataset importer loads GiantSteps Tempo v2 and GiantSteps Key labels into
  provenance-aware benchmark manifests.
- Test Kitchen edits validated private manifests with audio playback, tap
  tempo, BPM and key segments, combined beat-switch events, Serato
  observations, reviewer metadata, and on-demand Trail Mix predictions.

## Next milestone: time-varying analysis

### Beat Salad

- Improve local tempo tracking across abrupt changes and gradual ramps.
- Preserve a continuous beat grid through tempo transitions.
- Distinguish real tempo changes from half-time and double-time
  reinterpretations.
- Report start BPM, end BPM, boundary time, and confidence for each segment.

### Key Lime

- Replace the initial frame-level chroma with a higher-resolution HPCP where
  evaluation shows it improves boundaries or classification.
- Estimate tuning offset before key classification.
- Smooth unstable frame-level estimates without erasing short real sections.
- Detect key-change boundaries and return local key segments.
- Support uncertain, modal, and no-key sections instead of forcing every
  window into one of 24 major or minor keys.

### Structural beat-switch detection

- Calculate rhythmic, spectral, timbral, and chroma novelty over time.
- Detect arrangement changes even when BPM and key remain constant.
- Combine coincident structural, BPM, and key changes into one timeline event
  while preserving each change type.
- Avoid treating short fills, dropouts, and transitions as full beat switches.

### Unified timeline

- Add versioned tempo, key, and structural segments to the public API.
- Add change events containing the boundary time, changed properties,
  before-and-after values, and confidence.
- Keep the global BPM and key as convenient summaries derived from the full
  timeline.

## Evaluation

- Use GiantSteps Tempo as the primary global BPM benchmark.
- Use GiantSteps Key as the primary global key benchmark.
- Use Ballroom and SMC for beat-grid and difficult rhythm evaluation.
- Use Isophonics and Metric Modulations annotations for local key, beat, and
  structural changes.
- Maintain a private DJ-focused corpus with reference annotations, Trail Mix
  results, Serato results, and reproducible open-source baseline results.
- Include **GONE, GONE / THANK YOU** as a manually annotated beat-switch test
  case because its structural boundary also changes BPM and key.
- Report boundary-time error, before-and-after BPM error, local key accuracy,
  beat continuity, false boundaries, runtime, memory, and binary size.

## Implementation order

1. Extend the private manifest and annotation workflow for beat, tempo, key,
   and structural boundaries.
2. Add local Key Lime segmentation.
3. Strengthen Beat Salad transition tracking and beat continuity.
4. Add structural novelty and beat-switch detection.
5. Combine all results into the unified Trail Mix timeline.
6. Run tuning and held-out evaluations against datasets, Serato, and
   reproducible baselines.
7. Integrate a tagged Trail Mix release into Cueport after the accuracy and
   size gates pass.
