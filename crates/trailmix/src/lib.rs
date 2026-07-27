//! Trail Mix combines compact, offline audio analysis components.
//!
//! The public API accepts normalized mono PCM so applications can choose their
//! own decoder and avoid paying for codec dependencies they do not need.

use serde::{Deserialize, Serialize};

pub use beat_salad::{BeatAnalysis, BeatConfig, BeatPosition, TempoSegment};
pub use key_lime::{KeyAnalysis, KeyConfig, KeySegment, Mode, MusicalKey, PitchClass};
pub use sampler_platter::{WaveformColumn, WaveformOverview};

#[derive(Debug, Clone, Copy)]
pub struct AudioBuffer<'a> {
    pub samples: &'a [f32],
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub beat: BeatConfig,
    pub key: KeyConfig,
    pub waveform_columns: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            beat: BeatConfig::default(),
            key: KeyConfig::default(),
            waveform_columns: 1_500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    pub duration_seconds: f64,
    pub beat: BeatAnalysis,
    pub key: KeyAnalysis,
    pub waveform: WaveformOverview,
}

/// Run all Trail Mix analyzers over normalized mono PCM.
#[must_use]
pub fn analyze(audio: AudioBuffer<'_>, config: AnalysisConfig) -> Analysis {
    let duration_seconds = if audio.sample_rate == 0 {
        0.0
    } else {
        audio.samples.len() as f64 / f64::from(audio.sample_rate)
    };

    Analysis {
        version: 1,
        duration_seconds,
        beat: beat_salad::analyze(audio.samples, audio.sample_rate, config.beat),
        key: key_lime::analyze(audio.samples, audio.sample_rate, config.key),
        waveform: sampler_platter::generate_overview(
            audio.samples,
            audio.sample_rate,
            config.waveform_columns,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_all_components() {
        let sample_rate = 8_000;
        let samples = (0..sample_rate * 4)
            .map(|index| {
                (2.0 * std::f32::consts::PI * 440.0 * index as f32 / sample_rate as f32).sin()
                    * 0.25
            })
            .collect::<Vec<_>>();

        let analysis = analyze(
            AudioBuffer {
                samples: &samples,
                sample_rate,
            },
            AnalysisConfig {
                waveform_columns: 100,
                ..AnalysisConfig::default()
            },
        );

        assert_eq!(analysis.version, 1);
        assert_eq!(analysis.waveform.columns.len(), 100);
        assert!(analysis.key.key.is_some());
        assert!((analysis.duration_seconds - 4.0).abs() < f64::EPSILON);
    }
}
