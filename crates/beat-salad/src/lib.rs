//! Lightweight beat, tempo, and tempo-segment analysis.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatPosition {
    pub time_seconds: f64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoSegment {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub bpm: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatAnalysis {
    pub version: u32,
    pub global_bpm: Option<f32>,
    pub confidence: f32,
    pub beats: Vec<BeatPosition>,
    pub tempo_segments: Vec<TempoSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatConfig {
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub local_window_seconds: f32,
    pub local_hop_seconds: f32,
    pub segment_change_ratio: f32,
}

impl Default for BeatConfig {
    fn default() -> Self {
        Self {
            min_bpm: 60.0,
            max_bpm: 200.0,
            frame_size: 1_024,
            hop_size: 512,
            local_window_seconds: 12.0,
            local_hop_seconds: 6.0,
            segment_change_ratio: 0.04,
        }
    }
}

/// Analyze mono PCM and return global and local tempo estimates.
#[must_use]
pub fn analyze(samples: &[f32], sample_rate: u32, config: BeatConfig) -> BeatAnalysis {
    if sample_rate == 0
        || samples.len() < config.frame_size.saturating_mul(2)
        || config.hop_size == 0
        || config.min_bpm <= 0.0
        || config.max_bpm <= config.min_bpm
    {
        return empty_analysis();
    }

    let onset_envelope = energy_onset_envelope(samples, config.frame_size, config.hop_size);
    let envelope_rate = sample_rate as f32 / config.hop_size as f32;
    let Some(global) = estimate_tempo(
        &onset_envelope,
        envelope_rate,
        config.min_bpm,
        config.max_bpm,
    ) else {
        return empty_analysis();
    };

    let duration = samples.len() as f64 / f64::from(sample_rate);
    let beats = estimate_beat_positions(
        &onset_envelope,
        global.bpm,
        envelope_rate,
        config.hop_size,
        sample_rate,
        duration,
        global.confidence,
    );
    let tempo_segments =
        estimate_segments(&onset_envelope, envelope_rate, duration, config, global);

    BeatAnalysis {
        version: 2,
        global_bpm: Some(global.bpm),
        confidence: global.confidence,
        beats,
        tempo_segments,
    }
}

fn empty_analysis() -> BeatAnalysis {
    BeatAnalysis {
        version: 2,
        global_bpm: None,
        confidence: 0.0,
        beats: Vec::new(),
        tempo_segments: Vec::new(),
    }
}

fn energy_onset_envelope(samples: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    if samples.len() < frame_size {
        return Vec::new();
    }

    let mut energies = Vec::with_capacity((samples.len() - frame_size) / hop_size + 1);
    let mut start = 0;
    while start + frame_size <= samples.len() {
        let energy = samples[start..start + frame_size]
            .iter()
            .map(|sample| {
                let finite = if sample.is_finite() { *sample } else { 0.0 };
                finite * finite
            })
            .sum::<f32>()
            / frame_size as f32;
        energies.push(energy.sqrt());
        start += hop_size;
    }

    let mut previous = energies.first().copied().unwrap_or_default();
    let mut onset = Vec::with_capacity(energies.len());
    onset.push(0.0);
    for energy in energies.into_iter().skip(1) {
        onset.push((energy - previous).max(0.0));
        previous = energy;
    }

    let mean = onset.iter().sum::<f32>() / onset.len().max(1) as f32;
    for value in &mut onset {
        *value = (*value - mean * 0.25).max(0.0);
    }
    onset
}

#[derive(Debug, Clone, Copy)]
struct TempoEstimate {
    bpm: f32,
    confidence: f32,
}

fn estimate_tempo(
    onset: &[f32],
    envelope_rate: f32,
    min_bpm: f32,
    max_bpm: f32,
) -> Option<TempoEstimate> {
    if onset.len() < 4 || onset.iter().all(|value| *value <= f32::EPSILON) {
        return None;
    }

    let min_lag = ((60.0 * envelope_rate / max_bpm).ceil() as usize).max(1);
    let max_lag =
        ((60.0 * envelope_rate / min_bpm).floor() as usize).min(onset.len().saturating_sub(1));
    if min_lag > max_lag {
        return None;
    }

    let zero_lag_energy = onset.iter().map(|value| value * value).sum::<f32>();
    let correlation_end = (max_lag + 2).min(onset.len().saturating_sub(1));
    let correlations = (0..=correlation_end)
        .map(|lag| normalized_autocorrelation(onset, lag))
        .collect::<Vec<_>>();
    let interval_histogram = onset_interval_histogram(onset, min_lag, max_lag);
    let histogram_max = interval_histogram
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    let mut base_scores = vec![0.0; max_lag + 1];
    for (lag, base_score) in base_scores
        .iter_mut()
        .enumerate()
        .take(max_lag + 1)
        .skip(min_lag)
    {
        let neighborhood_start = lag.saturating_sub(1).max(1);
        let neighborhood_end = (lag + 1).min(max_lag);
        let interval_score = interval_histogram[neighborhood_start..=neighborhood_end]
            .iter()
            .sum::<f32>()
            / histogram_max;
        let periodicity = correlations[neighborhood_start..=neighborhood_end]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        *base_score = 0.65 * interval_score + 0.35 * periodicity;
    }

    let mut candidates = Vec::with_capacity(max_lag - min_lag + 1);
    for lag in min_lag..=max_lag {
        let mut family_score = base_scores[lag];
        for (multiple, weight) in [(2, 0.35), (3, 0.15)] {
            if let Some(family_lag) = lag.checked_mul(multiple)
                && family_lag <= max_lag
            {
                family_score += base_scores[family_lag] * weight;
            }
        }
        let bpm = 60.0 * envelope_rate / lag as f32;
        let octave_prior = (-(bpm / 120.0).log2().powi(2) / 2.0).exp();
        let score = family_score * (0.9 + 0.1 * octave_prior);
        candidates.push((lag, score));
    }

    let &(best_lag, best_score) = candidates
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let second_score = candidates
        .iter()
        .filter(|(lag, _)| lag.abs_diff(best_lag) > 2)
        .map(|(_, score)| *score)
        .fold(0.0_f32, f32::max);

    let refine_start = best_lag.saturating_sub(1).max(min_lag);
    let refine_end = (best_lag + 1).min(max_lag);
    let refine_weight = interval_histogram[refine_start..=refine_end]
        .iter()
        .sum::<f32>();
    let period = if refine_weight > f32::EPSILON {
        (refine_start..=refine_end)
            .map(|lag| lag as f32 * interval_histogram[lag])
            .sum::<f32>()
            / refine_weight
    } else {
        best_lag as f32
    };
    let bpm = (60.0 * envelope_rate / period).clamp(min_bpm, max_bpm);
    let periodicity = correlations[best_lag].clamp(0.0, 1.0);
    let separation = if best_score > f32::EPSILON {
        ((best_score - second_score.max(0.0)) / best_score).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let activity = (zero_lag_energy / onset.len() as f32).sqrt();
    let activity_gate = (activity * 100.0).clamp(0.0, 1.0);

    Some(TempoEstimate {
        bpm,
        confidence: (periodicity * (0.75 + 0.25 * separation) * activity_gate).clamp(0.0, 1.0),
    })
}

fn onset_interval_histogram(onset: &[f32], min_lag: usize, max_lag: usize) -> Vec<f32> {
    let mean = onset.iter().sum::<f32>() / onset.len().max(1) as f32;
    let variance = onset
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / onset.len().max(1) as f32;
    let threshold = mean + variance.sqrt() * 0.5;
    let peaks = (1..onset.len().saturating_sub(1))
        .filter(|&index| {
            onset[index] > threshold
                && onset[index] >= onset[index - 1]
                && onset[index] > onset[index + 1]
        })
        .collect::<Vec<_>>();

    let mut histogram = vec![0.0; max_lag + 1];
    for (peak_index, &current) in peaks.iter().enumerate() {
        for &previous in peaks[..peak_index].iter().rev() {
            let interval = current - previous;
            if interval > max_lag {
                break;
            }
            if interval >= min_lag {
                histogram[interval] += (onset[current] * onset[previous]).sqrt();
            }
        }
    }
    histogram
}

fn normalized_autocorrelation(onset: &[f32], lag: usize) -> f32 {
    if lag == 0 {
        return 1.0;
    }

    let mut cross = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for index in lag..onset.len() {
        let left = onset[index];
        let right = onset[index - lag];
        cross += left * right;
        left_energy += left * left;
        right_energy += right * right;
    }
    cross / (left_energy * right_energy).sqrt().max(f32::EPSILON)
}

#[allow(clippy::too_many_arguments)]
fn estimate_beat_positions(
    onset: &[f32],
    bpm: f32,
    envelope_rate: f32,
    hop_size: usize,
    sample_rate: u32,
    duration: f64,
    confidence: f32,
) -> Vec<BeatPosition> {
    let period = (60.0 * envelope_rate / bpm).round().max(1.0) as usize;
    let mut best_phase = 0;
    let mut best_score = f32::NEG_INFINITY;
    for phase in 0..period {
        let score = (phase..onset.len())
            .step_by(period)
            .map(|index| onset[index])
            .sum::<f32>();
        if score > best_score {
            best_score = score;
            best_phase = phase;
        }
    }

    (best_phase..onset.len())
        .step_by(period)
        .map(|frame| frame as f64 * hop_size as f64 / f64::from(sample_rate))
        .take_while(|time| *time <= duration)
        .map(|time_seconds| BeatPosition {
            time_seconds,
            confidence,
        })
        .collect()
}

fn estimate_segments(
    onset: &[f32],
    envelope_rate: f32,
    duration: f64,
    config: BeatConfig,
    global: TempoEstimate,
) -> Vec<TempoSegment> {
    let window_frames = (config.local_window_seconds * envelope_rate).round() as usize;
    let hop_frames = (config.local_hop_seconds * envelope_rate).round() as usize;
    if window_frames < 4 || hop_frames == 0 || onset.len() < window_frames + hop_frames {
        return vec![TempoSegment {
            start_seconds: 0.0,
            end_seconds: duration,
            bpm: global.bpm,
            confidence: global.confidence,
        }];
    }

    let mut local = Vec::new();
    let mut start = 0;
    while start + window_frames <= onset.len() {
        if let Some(estimate) = estimate_tempo(
            &onset[start..start + window_frames],
            envelope_rate,
            config.min_bpm,
            config.max_bpm,
        ) {
            let center = (start + window_frames / 2) as f64 / f64::from(envelope_rate);
            local.push((center, estimate));
        }
        start += hop_frames;
    }
    if local.is_empty() {
        return vec![TempoSegment {
            start_seconds: 0.0,
            end_seconds: duration,
            bpm: global.bpm,
            confidence: global.confidence,
        }];
    }

    let mut groups: Vec<(usize, usize)> = vec![(0, 1)];
    for index in 1..local.len() {
        let previous = local[index - 1].1.bpm;
        let current = local[index].1.bpm;
        let relative_change = (current - previous).abs() / previous.max(1.0);
        if relative_change > config.segment_change_ratio {
            groups.push((index, index + 1));
        } else if let Some(group) = groups.last_mut() {
            group.1 = index + 1;
        }
    }

    groups
        .iter()
        .enumerate()
        .map(|(group_index, &(first, end))| {
            let start_seconds = if group_index == 0 {
                0.0
            } else {
                f64::midpoint(local[first - 1].0, local[first].0)
            };
            let end_seconds = if end == local.len() {
                duration
            } else {
                f64::midpoint(local[end - 1].0, local[end].0)
            };
            let count = (end - first) as f32;
            TempoSegment {
                start_seconds,
                end_seconds,
                bpm: local[first..end]
                    .iter()
                    .map(|(_, estimate)| estimate.bpm)
                    .sum::<f32>()
                    / count,
                confidence: local[first..end]
                    .iter()
                    .map(|(_, estimate)| estimate.confidence)
                    .sum::<f32>()
                    / count,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_track(bpm: f32, seconds: f32, sample_rate: u32) -> Vec<f32> {
        let mut samples = vec![0.0; (seconds * sample_rate as f32) as usize];
        let interval = (60.0 / bpm * sample_rate as f32) as usize;
        for position in (0..samples.len()).step_by(interval.max(1)) {
            for offset in 0..128 {
                if let Some(sample) = samples.get_mut(position + offset) {
                    *sample = 1.0 - offset as f32 / 128.0;
                }
            }
        }
        samples
    }

    fn mixed_period_onset(length: usize, periods: &[(usize, f32)]) -> Vec<f32> {
        let mut onset = vec![0.0; length];
        for &(period, strength) in periods {
            for position in (0..length).step_by(period) {
                onset[position] += strength;
            }
        }
        onset
    }

    #[test]
    fn detects_a_120_bpm_click_track() {
        let samples = click_track(120.0, 20.0, 44_100);
        let result = analyze(&samples, 44_100, BeatConfig::default());
        let bpm = result.global_bpm.expect("tempo");

        assert!((bpm - 120.0).abs() < 2.0, "detected {bpm}");
        assert!(result.beats.len() >= 35);
        assert_eq!(result.tempo_segments.len(), 1);
    }

    #[test]
    fn keeps_tempo_inside_the_configured_range() {
        let samples = click_track(200.0, 20.0, 44_100);
        let config = BeatConfig {
            min_bpm: 80.0,
            max_bpm: 200.0,
            ..BeatConfig::default()
        };
        let result = analyze(&samples, 44_100, config);
        let bpm = result.global_bpm.expect("tempo");

        assert!((config.min_bpm..=config.max_bpm).contains(&bpm));
    }

    #[test]
    fn prefers_the_base_pulse_over_a_two_thirds_alias() {
        let envelope_rate = 44_100.0 / 512.0;
        let onset = mixed_period_onset(2_000, &[(30, 0.65), (45, 1.0)]);
        let estimate = estimate_tempo(&onset, envelope_rate, 60.0, 200.0).expect("tempo");

        assert!(
            (estimate.bpm - 172.27).abs() < 3.0,
            "detected {}",
            estimate.bpm
        );
    }

    #[test]
    fn detects_piecewise_tempo() {
        let mut samples = click_track(120.0, 24.0, 44_100);
        samples.extend(click_track(90.0, 24.0, 44_100));

        let result = analyze(&samples, 44_100, BeatConfig::default());

        assert!(
            result
                .tempo_segments
                .iter()
                .any(|segment| (segment.bpm - 120.0).abs() < 3.0),
            "{:?}",
            result.tempo_segments
        );
        assert!(
            result
                .tempo_segments
                .iter()
                .any(|segment| (segment.bpm - 90.0).abs() < 3.0),
            "{:?}",
            result.tempo_segments
        );
    }

    #[test]
    fn rejects_silence() {
        let result = analyze(&vec![0.0; 44_100 * 5], 44_100, BeatConfig::default());
        assert_eq!(result.global_bpm, None);
        assert!(result.beats.is_empty());
    }
}
