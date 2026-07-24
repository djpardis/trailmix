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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyConfig {
    pub frame_size: usize,
    pub hop_size: usize,
    pub minimum_midi_note: u8,
    pub maximum_midi_note: u8,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            frame_size: 4_096,
            hop_size: 2_048,
            minimum_midi_note: 36,
            maximum_midi_note: 95,
        }
    }
}

/// Estimate a global major or minor key from mono PCM.
#[must_use]
pub fn analyze(samples: &[f32], sample_rate: u32, config: KeyConfig) -> KeyAnalysis {
    if sample_rate == 0
        || config.frame_size < 32
        || config.hop_size == 0
        || samples.len() < config.frame_size
        || config.minimum_midi_note > config.maximum_midi_note
    {
        return empty_analysis();
    }

    let mut chroma = [0.0_f32; 12];
    let mut frame_count = 0_u32;
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
            accumulate_chroma(frame, sample_rate, config, &mut chroma);
            frame_count += 1;
        }
        start += config.hop_size;
    }

    let total = chroma.iter().sum::<f32>();
    if frame_count == 0 || total <= f32::EPSILON {
        return empty_analysis();
    }
    for value in &mut chroma {
        *value /= total;
    }

    let (key, best_score, second_score) = classify_key(&chroma);
    let confidence = if best_score.abs() > f32::EPSILON {
        ((best_score - second_score) / best_score.abs()).clamp(0.0, 1.0)
    } else {
        0.0
    };

    KeyAnalysis {
        version: 1,
        key: Some(key),
        confidence,
        chroma,
    }
}

fn empty_analysis() -> KeyAnalysis {
    KeyAnalysis {
        version: 1,
        key: None,
        confidence: 0.0,
        chroma: [0.0; 12],
    }
}

fn accumulate_chroma(frame: &[f32], sample_rate: u32, config: KeyConfig, chroma: &mut [f32; 12]) {
    let nyquist_guard = sample_rate as f32 * 0.45;
    for midi_note in config.minimum_midi_note..=config.maximum_midi_note {
        let frequency = 440.0 * 2.0_f32.powf((f32::from(midi_note) - 69.0) / 12.0);
        if frequency >= nyquist_guard {
            break;
        }

        let magnitude = goertzel_power(frame, sample_rate, frequency);
        let pitch_class = usize::from(midi_note % 12);
        chroma[pitch_class] += magnitude.sqrt() / frequency.sqrt();
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
    }

    #[test]
    fn rejects_silence() {
        let result = analyze(&vec![0.0; 44_100], 44_100, KeyConfig::default());
        assert_eq!(result.key, None);
    }
}
