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

    let mut chroma = [0.0_f32; 12];
    let mut frame_chromas = Vec::new();
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
            let mut frame_chroma = [0.0; 12];
            accumulate_chroma(frame, sample_rate, config, &mut frame_chroma);
            normalize_chroma(&mut frame_chroma);
            for (total, value) in chroma.iter_mut().zip(frame_chroma) {
                *total += value;
            }
            frame_chromas.push(FrameChroma {
                center_seconds: (start + config.frame_size / 2) as f64 / f64::from(sample_rate),
                values: frame_chroma,
            });
        }
        start += config.hop_size;
    }

    let total = chroma.iter().sum::<f32>();
    if frame_chromas.is_empty() || total <= f32::EPSILON {
        return empty_analysis();
    }
    normalize_chroma(&mut chroma);

    let (key, best_score, second_score) = classify_key(&chroma);
    let confidence = key_confidence(best_score, second_score);
    let duration_seconds = samples.len() as f64 / f64::from(sample_rate);
    let segments = estimate_segments(&frame_chromas, duration_seconds, key, confidence, config);
    let alternate =
        significant_alternate_key(&segments, key, config.alternate_coverage_threshold);

    KeyAnalysis {
        version: 4,
        key: Some(key),
        confidence,
        chroma,
        segments,
        multi_key: alternate.is_some(),
        alternate_key: alternate.map(|(key, _)| key),
        alternate_coverage: alternate.map_or(0.0, |(_, coverage)| coverage),
    }
}

fn empty_analysis() -> KeyAnalysis {
    KeyAnalysis {
        version: 4,
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
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|(key, _)| *key == segment.key)
        {
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

    let mut groups = vec![(0, 1)];
    for index in 1..local.len() {
        if local[index].key == local[index - 1].key {
            groups.last_mut().expect("initial group").1 = index + 1;
        } else {
            groups.push((index, index + 1));
        }
    }

    groups
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
            KeySegment {
                start_seconds,
                end_seconds,
                key: local[first].key,
                confidence: local[first..end]
                    .iter()
                    .map(|estimate| estimate.confidence)
                    .sum::<f32>()
                    / (end - first) as f32,
            }
        })
        .collect()
}

fn accumulate_chroma(frame: &[f32], sample_rate: u32, config: KeyConfig, chroma: &mut [f32; 12]) {
    let nyquist_guard = sample_rate as f32 * 0.45;
    let mut pitch_class_weights = [0.0; 12];
    for midi_note in config.minimum_midi_note..=config.maximum_midi_note {
        let frequency = 440.0 * 2.0_f32.powf((f32::from(midi_note) - 69.0) / 12.0);
        if frequency >= nyquist_guard {
            break;
        }

        let magnitude = goertzel_power(frame, sample_rate, frequency);
        let pitch_class = usize::from(midi_note % 12);
        let weight = 1.0 / frequency.sqrt();
        chroma[pitch_class] += magnitude.sqrt() * weight;
        pitch_class_weights[pitch_class] += weight;
    }

    normalize_pitch_class_weights(chroma, &pitch_class_weights);
}

fn normalize_pitch_class_weights(chroma: &mut [f32; 12], weights: &[f32; 12]) {
    for (value, weight) in chroma.iter_mut().zip(weights) {
        if *weight > f32::EPSILON {
            *value /= *weight;
        }
    }
}

fn goertzel_power(samples: &[f32], sample_rate: u32, frequency: f32) -> f32 {
    let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate as f32;
    let coefficient = 2.0 * omega.cos();
    let denominator = (samples.len().saturating_sub(1)).max(1) as f32;
    let mut previous = 0.0;
    let mut previous_previous = 0.0;

    for (index, raw_sample) in samples.iter().enumerate() {
        let sample = if raw_sample.is_finite() {
            *raw_sample
        } else {
            0.0
        };
        let window = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * index as f32 / denominator).cos();
        let current = sample * window + coefficient * previous - previous_previous;
        previous_previous = previous;
        previous = current;
    }

    (previous_previous * previous_previous + previous * previous
        - coefficient * previous * previous_previous)
        .max(0.0)
}

fn classify_key(chroma: &[f32; 12]) -> (MusicalKey, f32, f32) {
    const MAJOR_PROFILE: [f32; 12] = [
        6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
    ];
    const MINOR_PROFILE: [f32; 12] = [
        6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
    ];

    let mut candidates = Vec::with_capacity(24);
    for root in 0..12 {
        candidates.push((
            MusicalKey {
                tonic: PitchClass::ALL[root],
                mode: Mode::Major,
            },
            correlation(chroma, &MAJOR_PROFILE, root),
        ));
        candidates.push((
            MusicalKey {
                tonic: PitchClass::ALL[root],
                mode: Mode::Minor,
            },
            correlation(chroma, &MINOR_PROFILE, root),
        ));
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));

    (candidates[0].0, candidates[0].1, candidates[1].1)
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
    fn rejects_silence() {
        let result = analyze(&vec![0.0; 44_100], 44_100, KeyConfig::default());
        assert_eq!(result.key, None);
        assert!(!result.multi_key);
    }
}
