# Architecture

trail mix accepts finite mono `f32` PCM plus a sample rate. Decoding, file
metadata, databases, application state, and network transport remain caller
responsibilities. This boundary keeps the analysis crates usable in desktop,
command-line, and research tools without coupling them to one application.

## Crates

- `trailmix` is the facade and versioned aggregate result.
- `beat-salad` estimates tempo, beat positions, and windowed tempo segments.
  - onset detection: uses 24-band spectral flux rather than simple RMS energy
    differencing. Goertzel filters (the same efficient single-frequency
    approach used by key-lime's chroma) are spaced logarithmically from 50 Hz
    to 11 kHz. The onset-strength function sums only positive per-band energy
    increases across frames. This detects actual musical events (note onsets,
    chord changes, hi-hats over sustained bass) that pure volume differencing
    misses. See Bello et al. (2005).
  - beat tracking: uses dynamic programming (Ellis 2007) instead of a fixed
    phase-locked grid. For each onset-strength frame, the tracker finds the
    predecessor beat that maximizes cumulative onset strength plus a Gaussian
    penalty for deviation from the expected inter-beat interval, then
    backtraces from the best endpoint. Beats snap to where musical events
    occur while the penalty maintains tempo consistency. This follows swing,
    rubato, and live-performance timing that a rigid grid cannot.
  - tempo estimation applies a gentle octave prior (sigma=2 octaves, centered
    on 120 BPM) to reduce half/double errors without penalizing fast tempos
    (170+ BPM).
  - `BeatPosition` includes a `position_in_bar` field (1-4, assumes 4/4 meter)
    for downbeat inference.
  - an optional `onnx-beat` feature enables ONNX-based beat tracking using
    external models for development comparison only. It is not used in
    production: the external model requires a large runtime dependency, cannot
    target WASM or embedded platforms, and does not align with the project's
    goal of lightweight, self-contained DSP. The feature exists so that
    `trailmix-bench` can measure how the heuristic pipeline compares with
    neural approaches on the same corpus.
- `key-lime` builds frame-level pitch-class profiles using dual chroma
  extraction (Goertzel + CQT) and classifies key at global and segment levels.
  - Dual chroma extraction: each frame computes chroma via both fixed-window
    Goertzel filters (good harmonic-series resolution) and variable-window
    Constant-Q Transform (better semitone separation at low frequencies). The
    two normalized chroma vectors are averaged before aggregation, capturing
    complementary spectral information. See Brown (1991) for CQT foundations
    and Schorkhuber and Klapuri (2010) for efficient CQT implementations.
  - Harmonic summation (Goertzel path): for each fundamental note, energy from
    its 2nd, 3rd, and 4th harmonics (at +12, +19, +24 semitones) is summed
    with configurable weights.
  - CQT path: uses frequency-dependent window lengths (Q * sr / freq) so low
    notes get longer windows and high notes shorter ones, giving uniform
    semitone resolution across all octaves.
  - Multiple key profiles: scores chroma against Krumhansl-Kessler (1982),
    Temperley (2001), EDMA (Faraldo et al. 2016), and corpus-learned profiles,
    picking the best correlation across all candidates.
  - Tuning estimation: detects sub-semitone pitch offset and shifts chroma
    before classification.
  - Median chroma aggregation: global chroma takes the per-pitch-class median
    across onset-weighted frames, suppressing transient events that only
    dominate a fraction of frames.
  - Spectral whitening: power-law compression of chroma bins before
    normalization prevents dominant frequencies from overwhelming the vector.
  - Relative-key disambiguation: when the top two candidates are relative keys
    and their scores are close, the key whose tonic has more chroma energy wins.
  - Confidence-gated segmentation: local windows below a confidence threshold
    do not create segment boundaries; short segments merge into neighbors.
  - Segment-majority voting: global key uses the longest segment's key when
    multiple segments exist.
- `sampler-platter` bins PCM into min, max, and RMS waveform columns.
- `trailmix-codecs` provides separately selectable common-format decoders and
  downmixes decoded channels to mono PCM.
- `trailmix-manifest` defines and validates local evaluation-corpus annotations.
- `trailmix-datasets` imports GiantSteps Tempo v2 and GiantSteps Key reference
  annotations into the shared manifest while retaining dataset provenance.
- `trailmix-cli` analyzes one supported audio file for manual evaluation.
- `trailmix-bench` emits machine-readable synthetic or manifest-driven
  real-track benchmark results.
- `test-kitchen` serves a loopback-only browser interface for playback,
  annotation, Serato observations, and on-demand analysis.

## Design rationale for dual chroma

Fixed-window Goertzel filters have constant frequency resolution (sr / N). At
low pitches (e.g., C2 at 65 Hz), adjacent semitones are closer together than
the resolution limit, leading to spectral leakage between bins. The CQT uses
window lengths proportional to pitch period, giving log-frequency resolution
that matches musical intervals.

However, CQT alone does not benefit from harmonic summation in the same way
(the per-note isolation makes summation redundant and can cause
double-counting). Goertzel with harmonic summation captures harmonic series
relationships effectively. Combining both, after independent whitening and
normalization, produces features with lower correlated errors than either method
alone.

## Output versions

Each analysis result records the analyzer version that produced it. If an app
stores results in a database, it can later tell which files were analyzed with
old logic and refresh only those rows.

## Current limitations

- Tempo estimation uses spectral-flux onset detection and dynamic programming
  beat tracking, not a trained model.
- Tempo segments represent locally stable estimates and do not yet model a
  continuous ramp between two BPM values. When a secondary tempo covers at
  least 25% of the file (configurable), beat salad sets `multi_tempo` and
  fills `alternate_bpm` so apps can mark the primary BPM and show the other
  song's tempo after a beat switch. key lime does the same with `multi_key`
  and `alternate_key`. This is for mashups and medleys in one file, not for
  half vs double counting of one pulse.
- Key segmentation does not yet represent modal and no-key sections.
- The decoder supports MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 when
  the corresponding Cargo features are enabled.
- Confidence values are preliminary and have not been calibrated on held-out
  recordings.

## References

- Bello, J. P. et al. (2005). [A tutorial on onset detection in music
  signals](https://doi.org/10.1109/TSA.2005.851998). IEEE Transactions on
  Speech and Audio Processing.
- Brown, J. C. (1991). [Calculation of a constant Q spectral
  transform](https://doi.org/10.1121/1.400476). JASA.
- Krumhansl, C. L. and Kessler, E. J. (1982). [Tracing the dynamic changes in
  perceived tonal organization](https://doi.org/10.1037/0033-295x.89.4.334).
  Psychological Review.
- Temperley, D. (2001). [The Cognition of Basic Musical
  Structures](https://mitpress.mit.edu/9780262201346/the-cognition-of-basic-musical-structures/).
  MIT Press.
- Faraldo, A. et al. (2016). [Key estimation in electronic dance
  music](https://doi.org/10.1007/978-3-319-30671-1_25). ECIR.
- Ellis, D. P. W. (2007). [Beat tracking by dynamic
  programming](https://doi.org/10.1080/09298210701653344). Journal of New
  Music Research.
- Schorkhuber, C. and Klapuri, A. (2010). [Constant-Q transform toolbox for
  music processing](https://doi.org/10.5281/zenodo.849741). SMC.
