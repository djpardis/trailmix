//! Compact chroma-based musical key estimation.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PitchClass {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl PitchClass {
    const ALL: [Self; 12] = [
        Self::C,
        Self::CSharp,
        Self::D,
        Self::DSharp,
        Self::E,
        Self::F,
        Self::FSharp,
        Self::G,
        Self::GSharp,
        Self::A,
        Self::ASharp,
        Self::B,
    ];
}

impl fmt::Display for PitchClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::C => "C",
            Self::CSharp => "C#",
            Self::D => "D",
            Self::DSharp => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::FSharp => "F#",
            Self::G => "G",
            Self::GSharp => "G#",
            Self::A => "A",
            Self::ASharp => "A#",
            Self::B => "B",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Major,
    Minor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicalKey {
    pub tonic: PitchClass,
    pub mode: Mode,
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {}",
            self.tonic,
            match self.mode {
                Mode::Major => "major",
                Mode::Minor => "minor",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyAnalysis {
    pub version: u32,
    pub key: Option<MusicalKey>,
    pub confidence: f32,
    pub chroma: [f32; 12],
    pub segments: Vec<KeySegment>,
    /// True when another key in the file covers enough duration to matter
    /// (mashup / edit / medley), not a scoring tie between nearby keys.
    /// Apps can mark the primary key (for example with a star) and show `alternate_key`.
    pub multi_key: bool,
    /// Other song/section key when `multi_key` is true; otherwise `None`.
    pub alternate_key: Option<MusicalKey>,
    /// Fraction of file duration covered by `alternate_key` (0.0 when none).
    pub alternate_coverage: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeySegment {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub key: MusicalKey,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyConfig {
    pub frame_size: usize,
    pub hop_size: usize,
    pub minimum_midi_note: u8,
    pub maximum_midi_note: u8,
    pub local_window_seconds: f32,
    pub local_hop_seconds: f32,
    /// Minimum fraction of file duration another key must cover (beat switch /
    /// multi-song file) before `multi_key` is set. Default 0.25.
    pub alternate_coverage_threshold: f32,
    /// Minimum confidence a local window must have to create a new segment
    /// boundary. Windows below this merge into the previous segment. Default 0.15.
    pub segment_confidence_threshold: f32,
    /// Minimum segment duration in seconds. Segments shorter than this are
    /// absorbed by their longest neighbor. Default 8.0.
    pub minimum_segment_seconds: f32,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            frame_size: 4_096,
            hop_size: 2_048,
            minimum_midi_note: 36,
            maximum_midi_note: 95,
            local_window_seconds: 12.0,
            local_hop_seconds: 6.0,
            alternate_coverage_threshold: 0.25,
            segment_confidence_threshold: 0.35,
            minimum_segment_seconds: 12.0,
        }
    }
}

#[derive(Debug, Clone)]
struct FrameChroma {
    center_seconds: f64,
    values: [f32; 12],
}

#[derive(Debug, Clone, Copy)]
struct LocalEstimate {
    center_seconds: f64,
    key: MusicalKey,
    confidence: f32,
}

/// Estimate global and local major or minor keys from mono PCM.
#[must_use]
pub fn analyze(samples: &[f32], sample_rate: u32, config: KeyConfig) -> KeyAnalysis {
    if sample_rate == 0
        || config.frame_size < 32
        || config.hop_size == 0
        || samples.len() < config.frame_size
        || config.minimum_midi_note > config.maximum_midi_note
        || config.local_window_seconds <= 0.0
        || config.local_hop_seconds <= 0.0
    {
        return empty_analysis();
    }

    let window = hanning_window(config.frame_size);
    let goertzel_table = GoertzelTable::new(sample_rate, config);
    let mut windowed_frame = vec![0.0_f32; config.frame_size];
    let mut chroma = [0.0_f32; 12];
    let mut frame_chromas = Vec::new();
    let mut frame_energies = Vec::new();
    let mut start = 0;
    while start + config.frame_size <= samples.len() {
        let frame = &samples[start..start + config.frame_size];
        let frame_energy = frame
            .iter()
            .filter(|sample| sample.is_finite())
            .map(|sample| sample * sample)
            .sum::<f32>()
            / config.frame_size as f32;

        if frame_energy > 1.0e-8 {
            apply_window(frame, &window, &mut windowed_frame);
            let mut frame_chroma = [0.0; 12];
            accumulate_chroma_windowed(&windowed_frame, &goertzel_table, &mut frame_chroma);
            whiten_chroma(&mut frame_chroma);
            normalize_chroma(&mut frame_chroma);
            frame_chromas.push(FrameChroma {
                center_seconds: (start + config.frame_size / 2) as f64 / f64::from(sample_rate),
                values: frame_chroma,
            });
            frame_energies.push(frame_energy);
        }
        start += config.hop_size;
    }

    if frame_chromas.is_empty() {
        return empty_analysis();
    }

    let onset_weights = compute_onset_weights(&frame_energies);
    let num_frames = frame_chromas.len();
    let median_idx = num_frames / 2;
    for (pitch_class, bin) in chroma.iter_mut().enumerate() {
        let mut values: Vec<f32> = frame_chromas
            .iter()
            .zip(onset_weights.iter())
            .map(|(frame, weight)| frame.values[pitch_class] * weight)
            .collect();
        let (_, median, _) = values.select_nth_unstable_by(median_idx, f32::total_cmp);
        *bin = *median;
    }

    let total = chroma.iter().sum::<f32>();
    if frame_chromas.is_empty() || total <= f32::EPSILON {
        return empty_analysis();
    }

    let tuning_offset = estimate_tuning(&frame_chromas);
    if tuning_offset.abs() > 0.01 {
        chroma = shift_chroma(&chroma, tuning_offset);
    }
    normalize_chroma(&mut chroma);

    let (key, best_score, second_score) = classify_key(&chroma);
    let confidence = key_confidence(best_score, second_score);
    let duration_seconds = samples.len() as f64 / f64::from(sample_rate);
    let segments = estimate_segments(&frame_chromas, duration_seconds, key, confidence, config);

    let final_key = if segments.len() > 1 {
        longest_segment_key(&segments).unwrap_or(key)
    } else {
        key
    };

    let alternate =
        significant_alternate_key(&segments, final_key, config.alternate_coverage_threshold);

    KeyAnalysis {
        version: 6,
        key: Some(final_key),
        confidence,
        chroma,
        segments,
        multi_key: alternate.is_some(),
        alternate_key: alternate.map(|(key, _)| key),
        alternate_coverage: alternate.map_or(0.0, |(_, coverage)| coverage),
    }
}

/// Return the key of the longest segment by duration.
fn longest_segment_key(segments: &[KeySegment]) -> Option<MusicalKey> {
    segments
        .iter()
        .max_by(|a, b| {
            let dur_a = a.end_seconds - a.start_seconds;
            let dur_b = b.end_seconds - b.start_seconds;
            dur_a.total_cmp(&dur_b)
        })
        .map(|seg| seg.key)
}

fn empty_analysis() -> KeyAnalysis {
    KeyAnalysis {
        version: 6,
        key: None,
        confidence: 0.0,
        chroma: [0.0; 12],
        segments: Vec::new(),
        multi_key: false,
        alternate_key: None,
        alternate_coverage: 0.0,
    }
}

fn significant_alternate_key(
    segments: &[KeySegment],
    primary: MusicalKey,
    coverage_threshold: f32,
) -> Option<(MusicalKey, f32)> {
    if coverage_threshold <= 0.0 || segments.is_empty() {
        return None;
    }

    let total_duration = segments
        .iter()
        .map(|segment| (segment.end_seconds - segment.start_seconds).max(0.0))
        .sum::<f64>();
    if total_duration <= f64::EPSILON {
        return None;
    }

    let mut clusters: Vec<(MusicalKey, f64)> = Vec::new();
    for segment in segments {
        let duration = (segment.end_seconds - segment.start_seconds).max(0.0);
        if duration <= f64::EPSILON || segment.key == primary {
            continue;
        }
        if let Some(cluster) = clusters.iter_mut().find(|(key, _)| *key == segment.key) {
            cluster.1 += duration;
        } else {
            clusters.push((segment.key, duration));
        }
    }

    let (key, duration) = clusters
        .into_iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let coverage = (duration / total_duration) as f32;
    if coverage + f32::EPSILON < coverage_threshold {
        return None;
    }
    Some((key, coverage))
}

/// Compute per-frame weights that emphasize onsets (energy increases).
/// Each frame gets a base weight of 1.0, plus a bonus proportional to positive
/// energy flux. This makes attack moments contribute more to global chroma.
fn compute_onset_weights(energies: &[f32]) -> Vec<f32> {
    if energies.is_empty() {
        return Vec::new();
    }
    let mut weights = vec![1.0_f32; energies.len()];
    for i in 1..energies.len() {
        let flux = (energies[i].sqrt() - energies[i - 1].sqrt()).max(0.0);
        weights[i] = 1.0 + flux * 4.0;
    }
    weights
}

/// Estimate global tuning offset in fractional pitch-class bins (-0.5 to +0.5).
/// Finds the weighted-average deviation of energy peaks from integer bin centers
/// across all frames. A result of +0.3 means the track is tuned ~30 cents sharp.
fn estimate_tuning(frames: &[FrameChroma]) -> f32 {
    if frames.is_empty() {
        return 0.0;
    }
    let mut weight_sum = 0.0_f32;
    let mut offset_sum = 0.0_f32;

    for frame in frames {
        for bin in 0..12 {
            let prev = frame.values[(bin + 11) % 12];
            let center = frame.values[bin];
            let next = frame.values[(bin + 1) % 12];
            if center > prev && center > next && center > f32::EPSILON {
                let denominator = 2.0 * center - prev - next;
                if denominator > f32::EPSILON {
                    let offset = 0.5 * (next - prev) / denominator;
                    weight_sum += center;
                    offset_sum += offset * center;
                }
            }
        }
    }

    if weight_sum > f32::EPSILON {
        (offset_sum / weight_sum).clamp(-0.5, 0.5)
    } else {
        0.0
    }
}

/// Shift chroma by a fractional bin amount using linear interpolation.
fn shift_chroma(chroma: &[f32; 12], offset: f32) -> [f32; 12] {
    let mut shifted = [0.0_f32; 12];
    for (bin, out) in shifted.iter_mut().enumerate() {
        let source = bin as f32 - offset;
        let lower = ((source.floor() as i32).rem_euclid(12)) as usize;
        let upper = (lower + 1) % 12;
        let fraction = source - source.floor();
        *out = chroma[lower] * (1.0 - fraction) + chroma[upper] * fraction;
    }
    shifted
}

/// Power-law compression of chroma bins to flatten spectral dominance.
/// A gamma of 0.5 (square root) prevents a single strong pitch class
/// (typically bass in EDM) from dominating the entire vector.
fn whiten_chroma(chroma: &mut [f32; 12]) {
    const GAMMA: f32 = 0.5;
    for value in chroma.iter_mut() {
        if *value > 0.0 {
            *value = value.powf(GAMMA);
        }
    }
}

fn normalize_chroma(chroma: &mut [f32; 12]) {
    let total = chroma.iter().sum::<f32>();
    if total > f32::EPSILON {
        for value in chroma {
            *value /= total;
        }
    }
}

fn key_confidence(best_score: f32, second_score: f32) -> f32 {
    if best_score.abs() > f32::EPSILON {
        ((best_score - second_score) / best_score.abs()).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn estimate_segments(
    frames: &[FrameChroma],
    duration_seconds: f64,
    global_key: MusicalKey,
    global_confidence: f32,
    config: KeyConfig,
) -> Vec<KeySegment> {
    let window = f64::from(config.local_window_seconds);
    let hop = f64::from(config.local_hop_seconds);
    if duration_seconds < window + hop {
        return vec![KeySegment {
            start_seconds: 0.0,
            end_seconds: duration_seconds,
            key: global_key,
            confidence: global_confidence,
        }];
    }

    let mut local = Vec::new();
    let mut window_start = 0.0;
    while window_start + window <= duration_seconds {
        let window_end = window_start + window;
        let mut chroma = [0.0; 12];
        let mut frame_count = 0;
        for frame in frames.iter().filter(|frame| {
            frame.center_seconds >= window_start && frame.center_seconds < window_end
        }) {
            for (total, value) in chroma.iter_mut().zip(frame.values) {
                *total += value;
            }
            frame_count += 1;
        }
        if frame_count > 0 {
            normalize_chroma(&mut chroma);
            let (key, best_score, second_score) = classify_key(&chroma);
            local.push(LocalEstimate {
                center_seconds: window_start + window / 2.0,
                key,
                confidence: key_confidence(best_score, second_score),
            });
        }
        window_start += hop;
    }

    if local.is_empty() {
        return vec![KeySegment {
            start_seconds: 0.0,
            end_seconds: duration_seconds,
            key: global_key,
            confidence: global_confidence,
        }];
    }

    let conf_threshold = config.segment_confidence_threshold;
    let mut groups = vec![(0, 1)];
    for index in 1..local.len() {
        let same_key = local[index].key == local[index - 1].key;
        let confident_change = !same_key && local[index].confidence >= conf_threshold;
        if confident_change {
            groups.push((index, index + 1));
        } else {
            groups.last_mut().expect("initial group").1 = index + 1;
        }
    }

    let min_duration = f64::from(config.minimum_segment_seconds);
    let mut segments: Vec<KeySegment> = groups
        .iter()
        .enumerate()
        .map(|(group_index, &(first, end))| {
            let start_seconds = if group_index == 0 {
                0.0
            } else {
                f64::midpoint(local[first - 1].center_seconds, local[first].center_seconds)
            };
            let end_seconds = if end == local.len() {
                duration_seconds
            } else {
                f64::midpoint(local[end - 1].center_seconds, local[end].center_seconds)
            };
            let avg_confidence = local[first..end]
                .iter()
                .map(|estimate| estimate.confidence)
                .sum::<f32>()
                / (end - first) as f32;
            let majority_key = majority_key_in_range(&local[first..end]);
            KeySegment {
                start_seconds,
                end_seconds,
                key: majority_key,
                confidence: avg_confidence,
            }
        })
        .collect();

    merge_short_segments(&mut segments, min_duration);

    segments
}

/// Find the key that covers the most estimates in a range (by count).
fn majority_key_in_range(estimates: &[LocalEstimate]) -> MusicalKey {
    let mut counts: Vec<(MusicalKey, usize)> = Vec::new();
    for est in estimates {
        if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == est.key) {
            entry.1 += 1;
        } else {
            counts.push((est.key, 1));
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(estimates[0].key, |(key, _)| key)
}

/// Merge segments shorter than `min_duration` into their longest neighbor.
fn merge_short_segments(segments: &mut Vec<KeySegment>, min_duration: f64) {
    loop {
        let short_idx = segments.iter().position(|seg| {
            (seg.end_seconds - seg.start_seconds) < min_duration && segments.len() > 1
        });
        let Some(idx) = short_idx else {
            break;
        };
        let merge_into = if idx == 0 {
            1
        } else if idx == segments.len() - 1 {
            idx - 1
        } else {
            let prev_dur = segments[idx - 1].end_seconds - segments[idx - 1].start_seconds;
            let next_dur = segments[idx + 1].end_seconds - segments[idx + 1].start_seconds;
            if prev_dur >= next_dur {
                idx - 1
            } else {
                idx + 1
            }
        };
        let (keep, remove) = if merge_into < idx {
            (merge_into, idx)
        } else {
            (idx, merge_into)
        };
        segments[keep].end_seconds = segments[remove].end_seconds.max(segments[keep].end_seconds);
        segments[keep].start_seconds = segments[remove]
            .start_seconds
            .min(segments[keep].start_seconds);
        segments.remove(remove);
    }
}

fn hanning_window(size: usize) -> Vec<f32> {
    let denominator = (size.saturating_sub(1)).max(1) as f32;
    (0..size)
        .map(|index| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * index as f32 / denominator).cos())
        .collect()
}

fn apply_window(frame: &[f32], window: &[f32], output: &mut [f32]) {
    for ((out, sample), w) in output.iter_mut().zip(frame.iter()).zip(window.iter()) {
        *out = if sample.is_finite() { *sample * w } else { 0.0 };
    }
}

struct GoertzelNote {
    coefficient: f32,
    pitch_class: usize,
    weight: f32,
    midi_note: u8,
}

struct HarmonicLink {
    harmonic_index: usize,
    weight: f32,
}

struct GoertzelTable {
    notes: Vec<GoertzelNote>,
    /// For each note index (as a potential fundamental), the indices of its
    /// harmonics in the notes array with their summation weights.
    harmonics_of: Vec<Vec<HarmonicLink>>,
    pitch_class_weights: [f32; 12],
}

impl GoertzelTable {
    fn new(sample_rate: u32, config: KeyConfig) -> Self {
        let nyquist_guard = sample_rate as f32 * 0.45;
        let mut notes = Vec::new();
        let mut pitch_class_weights = [0.0_f32; 12];
        for midi_note in config.minimum_midi_note..=config.maximum_midi_note {
            let frequency = 440.0 * 2.0_f32.powf((f32::from(midi_note) - 69.0) / 12.0);
            if frequency >= nyquist_guard {
                break;
            }
            let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate as f32;
            let pitch_class = usize::from(midi_note % 12);
            let weight = 1.0 / frequency.sqrt();
            pitch_class_weights[pitch_class] += weight;
            notes.push(GoertzelNote {
                coefficient: 2.0 * omega.cos(),
                pitch_class,
                weight,
                midi_note,
            });
        }

        let harmonics_of = Self::build_harmonic_links(&notes);

        Self {
            notes,
            harmonics_of,
            pitch_class_weights,
        }
    }

    fn build_harmonic_links(notes: &[GoertzelNote]) -> Vec<Vec<HarmonicLink>> {
        const HARMONIC_SEMITONES: [(u8, f32); 3] = [
            (12, 0.50), // 2nd harmonic: +12 semitones (octave)
            (19, 0.33), // 3rd harmonic: +19 semitones (octave + fifth)
            (24, 0.25), // 4th harmonic: +24 semitones (two octaves)
        ];

        let mut links: Vec<Vec<HarmonicLink>> = (0..notes.len()).map(|_| Vec::new()).collect();
        for (fund_idx, fund_note) in notes.iter().enumerate() {
            for &(semitones, weight) in &HARMONIC_SEMITONES {
                let harmonic_midi = fund_note.midi_note.saturating_add(semitones);
                if let Some(harm_idx) = notes.iter().position(|n| n.midi_note == harmonic_midi) {
                    links[fund_idx].push(HarmonicLink {
                        harmonic_index: harm_idx,
                        weight,
                    });
                }
            }
        }
        links
    }
}

fn accumulate_chroma_windowed(
    windowed_frame: &[f32],
    table: &GoertzelTable,
    chroma: &mut [f32; 12],
) {
    let notes = &table.notes;
    let note_count = notes.len();
    let remainder_start = note_count - (note_count % 4);

    let mut magnitudes = vec![0.0_f32; note_count];

    for (chunk_idx, chunk) in notes.chunks_exact(4).enumerate() {
        let powers = goertzel_power_batch_4(
            windowed_frame,
            [
                chunk[0].coefficient,
                chunk[1].coefficient,
                chunk[2].coefficient,
                chunk[3].coefficient,
            ],
        );
        let base = chunk_idx * 4;
        magnitudes[base] = powers[0].sqrt();
        magnitudes[base + 1] = powers[1].sqrt();
        magnitudes[base + 2] = powers[2].sqrt();
        magnitudes[base + 3] = powers[3].sqrt();
    }
    for idx in remainder_start..note_count {
        let power = goertzel_power_prewindowed(windowed_frame, notes[idx].coefficient);
        magnitudes[idx] = power.sqrt();
    }

    for (idx, note) in notes.iter().enumerate() {
        let mut contribution = magnitudes[idx];
        for link in &table.harmonics_of[idx] {
            contribution += magnitudes[link.harmonic_index] * link.weight;
        }
        chroma[note.pitch_class] += contribution * note.weight;
    }
    normalize_pitch_class_weights(chroma, &table.pitch_class_weights);
}

fn normalize_pitch_class_weights(chroma: &mut [f32; 12], weights: &[f32; 12]) {
    for (value, weight) in chroma.iter_mut().zip(weights) {
        if *weight > f32::EPSILON {
            *value /= *weight;
        }
    }
}

fn goertzel_power_batch_4(samples: &[f32], coefficients: [f32; 4]) -> [f32; 4] {
    let mut prev = [0.0_f32; 4];
    let mut prev_prev = [0.0_f32; 4];

    for &sample in samples {
        for i in 0..4 {
            let current = sample + coefficients[i] * prev[i] - prev_prev[i];
            prev_prev[i] = prev[i];
            prev[i] = current;
        }
    }

    let mut powers = [0.0_f32; 4];
    for i in 0..4 {
        powers[i] = (prev_prev[i] * prev_prev[i] + prev[i] * prev[i]
            - coefficients[i] * prev[i] * prev_prev[i])
            .max(0.0);
    }
    powers
}

fn goertzel_power_prewindowed(samples: &[f32], coefficient: f32) -> f32 {
    let mut previous = 0.0_f32;
    let mut previous_previous = 0.0_f32;

    for &sample in samples {
        let current = sample + coefficient * previous - previous_previous;
        previous_previous = previous;
        previous = current;
    }

    (previous_previous * previous_previous + previous * previous
        - coefficient * previous * previous_previous)
        .max(0.0)
}

fn classify_key(chroma: &[f32; 12]) -> (MusicalKey, f32, f32) {
    const KRUMHANSL_MAJOR: [f32; 12] = [
        6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
    ];
    const KRUMHANSL_MINOR: [f32; 12] = [
        6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
    ];
    const TEMPERLEY_MAJOR: [f32; 12] = [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0];
    const TEMPERLEY_MINOR: [f32; 12] = [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0];
    const EDMA_MAJOR: [f32; 12] = [
        6.80, 3.00, 4.20, 2.80, 5.60, 4.40, 2.60, 5.80, 3.20, 4.40, 2.40, 3.80,
    ];
    const EDMA_MINOR: [f32; 12] = [
        6.60, 3.20, 4.00, 5.40, 3.00, 4.20, 2.80, 5.20, 4.40, 3.00, 3.60, 3.60,
    ];
    const LEARNED_MAJOR: [f32; 12] = [
        0.0972, 0.0776, 0.0833, 0.0754, 0.0850, 0.0818, 0.0749, 0.0922, 0.0743, 0.0804, 0.0766,
        0.0831,
    ];
    const LEARNED_MINOR: [f32; 12] = [
        0.0972, 0.0815, 0.0815, 0.0825, 0.0745, 0.0798, 0.0754, 0.0917, 0.0808, 0.0769, 0.0830,
        0.0819,
    ];

    let profiles: &[(&[f32; 12], &[f32; 12])] = &[
        (&KRUMHANSL_MAJOR, &KRUMHANSL_MINOR),
        (&TEMPERLEY_MAJOR, &TEMPERLEY_MINOR),
        (&EDMA_MAJOR, &EDMA_MINOR),
        (&LEARNED_MAJOR, &LEARNED_MINOR),
    ];

    let mut best_key = MusicalKey {
        tonic: PitchClass::C,
        mode: Mode::Major,
    };
    let mut second_key = best_key;
    let mut best_score = f32::NEG_INFINITY;
    let mut second_score = f32::NEG_INFINITY;

    for &(major_profile, minor_profile) in profiles {
        for root in 0..12 {
            let major_corr = correlation(chroma, major_profile, root);
            let minor_corr = correlation(chroma, minor_profile, root);

            for (mode, score) in [(Mode::Major, major_corr), (Mode::Minor, minor_corr)] {
                if score > best_score {
                    second_score = best_score;
                    second_key = best_key;
                    best_score = score;
                    best_key = MusicalKey {
                        tonic: PitchClass::ALL[root],
                        mode,
                    };
                } else if score > second_score {
                    second_score = score;
                    second_key = MusicalKey {
                        tonic: PitchClass::ALL[root],
                        mode,
                    };
                }
            }
        }
    }

    let final_key =
        disambiguate_relative_keys(chroma, best_key, second_key, best_score, second_score);
    (final_key, best_score, second_score)
}

fn disambiguate_relative_keys(
    chroma: &[f32; 12],
    best: MusicalKey,
    second: MusicalKey,
    best_score: f32,
    second_score: f32,
) -> MusicalKey {
    let margin = best_score - second_score;
    if margin > 0.05 {
        return best;
    }
    if !are_relative_keys(best, second) {
        return best;
    }
    let best_tonic_energy = chroma[best.tonic as usize];
    let second_tonic_energy = chroma[second.tonic as usize];
    if second_tonic_energy > best_tonic_energy * 1.2 {
        second
    } else {
        best
    }
}

fn are_relative_keys(a: MusicalKey, b: MusicalKey) -> bool {
    if a.mode == b.mode {
        return false;
    }
    let (major, minor) = if a.mode == Mode::Major {
        (a.tonic as usize, b.tonic as usize)
    } else {
        (b.tonic as usize, a.tonic as usize)
    };
    (major + 9) % 12 == minor
}

fn correlation(chroma: &[f32; 12], profile: &[f32; 12], root: usize) -> f32 {
    let chroma_mean = chroma.iter().sum::<f32>() / 12.0;
    let profile_mean = profile.iter().sum::<f32>() / 12.0;
    let mut numerator = 0.0;
    let mut chroma_energy = 0.0;
    let mut profile_energy = 0.0;

    for pitch_class in 0..12 {
        let chroma_value = chroma[pitch_class] - chroma_mean;
        let profile_value = profile[(pitch_class + 12 - root) % 12] - profile_mean;
        numerator += chroma_value * profile_value;
        chroma_energy += chroma_value * chroma_value;
        profile_energy += profile_value * profile_value;
    }

    numerator / (chroma_energy * profile_energy).sqrt().max(f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(frequencies: &[f32], seconds: f32, sample_rate: u32) -> Vec<f32> {
        let length = (seconds * sample_rate as f32) as usize;
        (0..length)
            .map(|index| {
                frequencies
                    .iter()
                    .map(|frequency| {
                        (2.0 * std::f32::consts::PI * frequency * index as f32 / sample_rate as f32)
                            .sin()
                    })
                    .sum::<f32>()
                    / frequencies.len() as f32
            })
            .collect()
    }

    #[test]
    fn detects_a_major_chord() {
        let samples = chord(&[220.0, 277.18, 329.63], 4.0, 44_100);
        let result = analyze(&samples, 44_100, KeyConfig::default());

        assert_eq!(
            result.key,
            Some(MusicalKey {
                tonic: PitchClass::A,
                mode: Mode::Major,
            })
        );
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].key, result.key.expect("key"));
        assert!(!result.multi_key);
        assert_eq!(result.alternate_key, None);
    }

    #[test]
    fn removes_frequency_weight_bias_between_pitch_classes() {
        let config = KeyConfig::default();
        let mut weights = [0.0; 12];
        for midi_note in config.minimum_midi_note..=config.maximum_midi_note {
            let frequency = 440.0 * 2.0_f32.powf((f32::from(midi_note) - 69.0) / 12.0);
            weights[usize::from(midi_note % 12)] += 1.0 / frequency.sqrt();
        }
        let mut chroma = weights;

        normalize_pitch_class_weights(&mut chroma, &weights);

        assert!(chroma.iter().all(|value| (*value - 1.0).abs() < 1.0e-6));
    }

    #[test]
    fn detects_a_piecewise_key_change() {
        let sample_rate = 8_000;
        let mut samples = chord(&[220.0, 277.18, 329.63], 8.0, sample_rate);
        samples.extend(chord(&[261.63, 311.13, 392.0], 8.0, sample_rate));

        let result = analyze(
            &samples,
            sample_rate,
            KeyConfig {
                frame_size: 2_048,
                hop_size: 1_024,
                local_window_seconds: 4.0,
                local_hop_seconds: 2.0,
                segment_confidence_threshold: 0.0,
                minimum_segment_seconds: 0.0,
                ..KeyConfig::default()
            },
        );

        assert_eq!(
            result.segments.first().expect("first segment").key.tonic,
            PitchClass::A
        );
        assert_eq!(
            result.segments.last().expect("last segment").key,
            MusicalKey {
                tonic: PitchClass::C,
                mode: Mode::Minor,
            }
        );
        assert!(
            result
                .segments
                .iter()
                .any(|segment| (segment.start_seconds - 8.0).abs() <= 2.0),
            "{:?}",
            result.segments
        );
        assert!(result.multi_key, "{result:?}");
        assert!(result.alternate_key.is_some());
        assert!(result.alternate_coverage >= 0.25);
    }

    #[test]
    fn alternate_key_respects_coverage_threshold() {
        let sample_rate = 8_000;
        let mut samples = chord(&[220.0, 277.18, 329.63], 8.0, sample_rate);
        samples.extend(chord(&[261.63, 311.13, 392.0], 8.0, sample_rate));

        let result = analyze(
            &samples,
            sample_rate,
            KeyConfig {
                frame_size: 2_048,
                hop_size: 1_024,
                local_window_seconds: 4.0,
                local_hop_seconds: 2.0,
                alternate_coverage_threshold: 0.75,
                segment_confidence_threshold: 0.0,
                minimum_segment_seconds: 0.0,
                ..KeyConfig::default()
            },
        );

        assert!(!result.multi_key);
        assert_eq!(result.alternate_key, None);
        assert_eq!(result.alternate_coverage, 0.0);
    }

    #[test]
    fn alternate_key_disabled_when_threshold_zero() {
        let sample_rate = 8_000;
        let mut samples = chord(&[220.0, 277.18, 329.63], 8.0, sample_rate);
        samples.extend(chord(&[261.63, 311.13, 392.0], 8.0, sample_rate));

        let result = analyze(
            &samples,
            sample_rate,
            KeyConfig {
                frame_size: 2_048,
                hop_size: 1_024,
                local_window_seconds: 4.0,
                local_hop_seconds: 2.0,
                alternate_coverage_threshold: 0.0,
                ..KeyConfig::default()
            },
        );
        assert!(!result.multi_key);
        assert_eq!(result.alternate_key, None);
    }

    #[test]
    fn significant_alternate_key_empty_segments() {
        let primary = MusicalKey {
            tonic: PitchClass::A,
            mode: Mode::Major,
        };
        assert_eq!(significant_alternate_key(&[], primary, 0.25), None);
    }

    #[test]
    fn significant_alternate_key_single_matching_segment() {
        let primary = MusicalKey {
            tonic: PitchClass::A,
            mode: Mode::Major,
        };
        let segments = vec![KeySegment {
            start_seconds: 0.0,
            end_seconds: 30.0,
            key: primary,
            confidence: 0.8,
        }];
        assert_eq!(significant_alternate_key(&segments, primary, 0.25), None);
    }

    #[test]
    fn significant_alternate_key_below_threshold() {
        let primary = MusicalKey {
            tonic: PitchClass::A,
            mode: Mode::Major,
        };
        let alternate = MusicalKey {
            tonic: PitchClass::C,
            mode: Mode::Minor,
        };
        let segments = vec![
            KeySegment {
                start_seconds: 0.0,
                end_seconds: 40.0,
                key: primary,
                confidence: 0.8,
            },
            KeySegment {
                start_seconds: 40.0,
                end_seconds: 48.0,
                key: alternate,
                confidence: 0.6,
            },
        ];
        // 8/48 = 16.7%, below 25%
        assert_eq!(significant_alternate_key(&segments, primary, 0.25), None);
    }

    #[test]
    fn significant_alternate_key_above_threshold() {
        let primary = MusicalKey {
            tonic: PitchClass::A,
            mode: Mode::Major,
        };
        let alternate = MusicalKey {
            tonic: PitchClass::C,
            mode: Mode::Minor,
        };
        let segments = vec![
            KeySegment {
                start_seconds: 0.0,
                end_seconds: 24.0,
                key: primary,
                confidence: 0.8,
            },
            KeySegment {
                start_seconds: 24.0,
                end_seconds: 48.0,
                key: alternate,
                confidence: 0.7,
            },
        ];
        let result = significant_alternate_key(&segments, primary, 0.25);
        assert!(result.is_some());
        let (key, coverage) = result.unwrap();
        assert_eq!(key, alternate);
        assert!((coverage - 0.5).abs() < 0.01);
    }

    #[test]
    fn detects_a_detuned_major_chord() {
        let detune_cents = 15.0;
        let detune_ratio = 2.0_f32.powf(detune_cents / 1200.0);
        let samples = chord(
            &[
                220.0 * detune_ratio,
                277.18 * detune_ratio,
                329.63 * detune_ratio,
            ],
            4.0,
            44_100,
        );
        let result = analyze(&samples, 44_100, KeyConfig::default());
        assert_eq!(
            result.key,
            Some(MusicalKey {
                tonic: PitchClass::A,
                mode: Mode::Major,
            }),
            "should detect A major despite 15-cent detuning, got {:?}",
            result.key
        );
    }

    #[test]
    fn rejects_silence() {
        let result = analyze(&vec![0.0; 44_100], 44_100, KeyConfig::default());
        assert_eq!(result.key, None);
        assert!(!result.multi_key);
    }
}
