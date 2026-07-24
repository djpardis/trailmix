//! Compact waveform summaries for display and storage.

use serde::{Deserialize, Serialize};

/// A single horizontal waveform column.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WaveformColumn {
    /// Lowest sample in the column.
    pub min: f32,
    /// Highest sample in the column.
    pub max: f32,
    /// Root mean square energy in the column.
    pub rms: f32,
}

/// A versioned waveform overview generated from mono PCM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveformOverview {
    pub version: u32,
    pub sample_rate: u32,
    pub source_samples: usize,
    pub columns: Vec<WaveformColumn>,
}

impl WaveformOverview {
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.source_samples as f64 / f64::from(self.sample_rate)
    }
}

/// Generate a min/max/RMS overview with at most `target_columns` columns.
///
/// Samples are expected to be finite, mono PCM values nominally in `-1.0..=1.0`.
/// Non-finite values are treated as silence.
#[must_use]
pub fn generate_overview(
    samples: &[f32],
    sample_rate: u32,
    target_columns: usize,
) -> WaveformOverview {
    if samples.is_empty() || target_columns == 0 {
        return WaveformOverview {
            version: 1,
            sample_rate,
            source_samples: samples.len(),
            columns: Vec::new(),
        };
    }

    let column_count = target_columns.min(samples.len());
    let mut columns = Vec::with_capacity(column_count);

    for column_index in 0..column_count {
        let start = column_index * samples.len() / column_count;
        let end = ((column_index + 1) * samples.len() / column_count).max(start + 1);
        let window = &samples[start..end];

        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum_squares = 0.0_f64;

        for &raw_sample in window {
            let sample = if raw_sample.is_finite() {
                raw_sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            min = min.min(sample);
            max = max.max(sample);
            sum_squares += f64::from(sample) * f64::from(sample);
        }

        columns.push(WaveformColumn {
            min,
            max,
            rms: (sum_squares / window.len() as f64).sqrt() as f32,
        });
    }

    WaveformOverview {
        version: 1,
        sample_rate,
        source_samples: samples.len(),
        columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_min_max_and_rms() {
        let overview = generate_overview(&[-1.0, 1.0, -0.5, 0.5], 4, 2);

        assert_eq!(overview.columns.len(), 2);
        assert_eq!(overview.columns[0].min, -1.0);
        assert_eq!(overview.columns[0].max, 1.0);
        assert!((overview.columns[0].rms - 1.0).abs() < f32::EPSILON);
        assert!((overview.duration_seconds() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn handles_empty_and_non_finite_input() {
        assert!(generate_overview(&[], 44_100, 100).columns.is_empty());

        let overview = generate_overview(&[f32::NAN, f32::INFINITY], 44_100, 1);
        assert_eq!(
            overview.columns[0],
            WaveformColumn {
                min: 0.0,
                max: 0.0,
                rms: 0.0,
            }
        );
    }
}
