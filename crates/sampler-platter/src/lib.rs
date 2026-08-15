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
    /// Display-oriented waveform height after track-level normalization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_height: Option<f32>,
    /// Approximate spectral balance, where 0 is bass-heavy and 1 is treble-heavy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectral_centroid: Option<f32>,
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
            display_height: None,
            spectral_centroid: Some(spectral_balance(window)),
        });
    }

    apply_display_heights(&mut columns);

    WaveformOverview {
        version: 1,
        sample_rate,
        source_samples: samples.len(),
        columns,
    }
}

fn apply_display_heights(columns: &mut [WaveformColumn]) {
    let energies = columns
        .iter()
        .map(|column| {
            let peak = column.min.abs().max(column.max.abs());
            if peak <= f32::EPSILON || column.rms <= f32::EPSILON {
                0.0
            } else {
                // The geometric mean keeps peak transients visible while
                // preserving RMS-driven differences inside dense music.
                (peak * column.rms).sqrt()
            }
        })
        .collect::<Vec<_>>();

    let max_energy = energies.iter().copied().fold(0.0_f32, f32::max);
    if max_energy <= f32::EPSILON {
        for column in columns {
            column.display_height = Some(0.0);
        }
        return;
    }

    let display_ceiling = percentile(energies.clone(), 0.95).max(max_energy * 0.5);

    for (column, energy) in columns.iter_mut().zip(energies) {
        column.display_height = Some(if energy <= f32::EPSILON {
            0.0
        } else {
            (energy / display_ceiling).clamp(0.0, 1.0).powf(0.60)
        });
    }
}

fn percentile(mut values: Vec<f32>, p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let idx = ((values.len() - 1) as f32 * p.clamp(0.0, 1.0)).round() as usize;
    values[idx]
}

fn spectral_balance(window: &[f32]) -> f32 {
    if window.len() < 2 {
        return 0.5;
    }

    let mut abs_sum = 0.0_f64;
    let mut delta_sum = 0.0_f64;
    let mut zero_crossings = 0usize;
    let mut previous = sanitize_sample(window[0]);

    abs_sum += f64::from(previous.abs());
    for &raw_sample in &window[1..] {
        let sample = sanitize_sample(raw_sample);
        abs_sum += f64::from(sample.abs());
        delta_sum += f64::from((sample - previous).abs());
        if (previous < 0.0 && sample >= 0.0) || (previous >= 0.0 && sample < 0.0) {
            zero_crossings += 1;
        }
        previous = sample;
    }

    if abs_sum <= f64::EPSILON {
        return 0.5;
    }

    let derivative_ratio = (delta_sum / (abs_sum * 2.0)).clamp(0.0, 1.0);
    let zero_crossing_ratio = (zero_crossings as f64 / (window.len() - 1) as f64).clamp(0.0, 1.0);
    (derivative_ratio.mul_add(0.7, zero_crossing_ratio * 0.3) as f32).clamp(0.0, 1.0)
}

fn sanitize_sample(raw_sample: f32) -> f32 {
    if raw_sample.is_finite() {
        raw_sample.clamp(-1.0, 1.0)
    } else {
        0.0
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
        assert!(overview.columns[0].spectral_centroid.is_some());
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
                display_height: Some(0.0),
                spectral_centroid: Some(0.5),
            }
        );
    }

    #[test]
    fn display_height_boosts_quiet_columns_without_clipping_loud_columns() {
        let overview = generate_overview(&[-0.02, 0.02, -1.0, 1.0], 4, 2);
        let quiet = overview.columns[0].display_height.unwrap();
        let loud = overview.columns[1].display_height.unwrap();

        assert!(quiet > 0.02);
        assert!(quiet < 0.5);
        assert!(quiet < loud);
        assert!(loud <= 1.0);
    }

    #[test]
    fn display_height_is_monotonic_with_signal_level() {
        let overview = generate_overview(&[-0.1, 0.1, -0.25, 0.25, -0.5, 0.5, -1.0, 1.0], 8, 4);
        let heights = overview
            .columns
            .iter()
            .map(|column| column.display_height.unwrap())
            .collect::<Vec<_>>();

        assert!(heights.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn spectral_balance_rises_with_fast_changes() {
        let slow = generate_overview(&[-1.0; 64], 44_100, 1);
        let fast_samples = (0..64)
            .map(|index| if index % 2 == 0 { -1.0 } else { 1.0 })
            .collect::<Vec<_>>();
        let fast = generate_overview(&fast_samples, 44_100, 1);

        assert!(
            fast.columns[0].spectral_centroid.unwrap() > slow.columns[0].spectral_centroid.unwrap()
        );
    }
}
