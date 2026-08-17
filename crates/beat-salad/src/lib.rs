//! Lightweight beat, tempo, and tempo-segment analysis.

#[cfg(feature = "onnx-beat")]
pub mod onnx_beat;
pub mod spectrogram;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatPosition {
    pub time_seconds: f64,
    pub confidence: f32,
    /// 1-based position within the bar (1 = downbeat). Assumes 4/4 meter.
    pub position_in_bar: u8,
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
    /// Tempo rounded for library display. This never replaces `global_bpm`,
    /// which remains the precise analyzer result for beatgrid math.
    pub display_bpm: Option<f32>,
    /// Number of decimal places the display BPM needs. Integer-like tempos use
    /// zero decimals; fractional estimates use one decimal.
    pub display_bpm_decimals: u8,
    pub confidence: f32,
    pub beats: Vec<BeatPosition>,
    pub tempo_segments: Vec<TempoSegment>,
    /// True when another tempo in the file covers enough duration to matter
    /// (mashup / edit / medley), not half vs double of one pulse.
    /// Apps can mark `global_bpm` (for example with a star) and show `alternate_bpm`.
    pub multi_tempo: bool,
    /// Other song/section BPM when `multi_tempo` is true; otherwise `None`.
    pub alternate_bpm: Option<f32>,
    /// Fraction of file duration covered by `alternate_bpm` (0.0 when none).
    pub alternate_coverage: f32,
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
    /// Minimum fraction of file duration another tempo must cover (beat switch /
    /// multi-song file) before `multi_tempo` is set. Default 0.25. Unrelated to
    /// half/double metrical ambiguity on a single pulse.
    pub alternate_coverage_threshold: f32,
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
            alternate_coverage_threshold: 0.25,
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
    let alternate = significant_alternate_tempo(
        &tempo_segments,
        global.bpm,
        config.segment_change_ratio,
        config.alternate_coverage_threshold,
    );

    BeatAnalysis {
        version: 4,
        global_bpm: Some(global.bpm),
        display_bpm: Some(display_bpm(global.bpm, &beats)),
        display_bpm_decimals: 0,
        confidence: global.confidence,
        beats,
        tempo_segments,
        multi_tempo: alternate.is_some(),
        alternate_bpm: alternate.map(|(bpm, _)| bpm),
        alternate_coverage: alternate.map_or(0.0, |(_, coverage)| coverage),
    }
}

fn empty_analysis() -> BeatAnalysis {
    BeatAnalysis {
        version: 4,
        global_bpm: None,
        display_bpm: None,
        display_bpm_decimals: 0,
        confidence: 0.0,
        beats: Vec::new(),
        tempo_segments: Vec::new(),
        multi_tempo: false,
        alternate_bpm: None,
        alternate_coverage: 0.0,
    }
}

fn bpm_matches(left: f32, right: f32, change_ratio: f32) -> bool {
    let scale = left.abs().max(right.abs()).max(f32::EPSILON);
    (left - right).abs() / scale <= change_ratio
}

fn display_bpm(bpm: f32, beats: &[BeatPosition]) -> f32 {
    let lower = bpm.floor().max(1.0);
    let upper = bpm.ceil().max(lower);
    let nearest = bpm.round().max(1.0);
    if beats.len() < 4 {
        return nearest;
    }

    [lower, upper]
        .into_iter()
        .min_by(|left, right| {
            let left_error = beat_grid_fit_error(*left, beats);
            let right_error = beat_grid_fit_error(*right, beats);
            left_error
                .total_cmp(&right_error)
                .then_with(|| (left - bpm).abs().total_cmp(&(right - bpm).abs()))
        })
        .unwrap_or(nearest)
}

fn beat_grid_fit_error(bpm: f32, beats: &[BeatPosition]) -> f64 {
    if bpm <= 0.0 || beats.len() < 2 {
        return f64::INFINITY;
    }
    let period = 60.0 / f64::from(bpm);
    let mut offsets = beats
        .iter()
        .enumerate()
        .map(|(idx, beat)| beat.time_seconds - idx as f64 * period)
        .collect::<Vec<_>>();
    let offset = median_f64(&mut offsets);
    beats
        .iter()
        .enumerate()
        .map(|(idx, beat)| {
            let expected = offset + idx as f64 * period;
            (beat.time_seconds - expected).abs()
        })
        .sum::<f64>()
        / beats.len() as f64
}

fn median_f64(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Longest secondary tempo cluster by duration, if it covers enough of the track.
fn significant_alternate_tempo(
    segments: &[TempoSegment],
    primary_bpm: f32,
    change_ratio: f32,
    coverage_threshold: f32,
) -> Option<(f32, f32)> {
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

    let mut clusters: Vec<(f32, f64)> = Vec::new();
    for segment in segments {
        let duration = (segment.end_seconds - segment.start_seconds).max(0.0);
        if duration <= f64::EPSILON || bpm_matches(segment.bpm, primary_bpm, change_ratio) {
            continue;
        }
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|(bpm, _)| bpm_matches(*bpm, segment.bpm, change_ratio))
        {
            let total = cluster.1 + duration;
            let weighted = f64::from(cluster.0) * cluster.1 + f64::from(segment.bpm) * duration;
            cluster.0 = (weighted / total.max(f64::EPSILON)) as f32;
            cluster.1 = total;
        } else {
            clusters.push((segment.bpm, duration));
        }
    }

    let (bpm, duration) = clusters
        .into_iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let coverage = (duration / total_duration) as f32;
    if coverage + f32::EPSILON < coverage_threshold {
        return None;
    }
    Some((bpm, coverage))
}

fn energy_onset_envelope(samples: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    if samples.len() < frame_size {
        return Vec::new();
    }

    // Multi-band spectral flux onset detection: compute energy in logarithmically
    // spaced frequency bands and measure positive changes between frames.
    // Uses 24 bands (2 per octave from ~50 Hz to ~11 kHz) for a good balance
    // of frequency sensitivity vs computation cost.
    let n_bands: usize = 24;
    let min_freq = 50.0f32;
    let max_freq = 11_000.0f32;
    let sample_rate = 44_100.0f32; // assumed; bands are relative anyway

    // Pre-compute Goertzel coefficients for each band center frequency
    let band_freqs: Vec<f32> = (0..n_bands)
        .map(|i| min_freq * (max_freq / min_freq).powf(i as f32 / (n_bands - 1) as f32))
        .collect();
    let band_coeffs: Vec<f32> = band_freqs
        .iter()
        .map(|&freq| 2.0 * (2.0 * std::f32::consts::PI * freq / sample_rate).cos())
        .collect();

    let window = hanning_window_beat(frame_size);
    let mut prev_magnitudes = vec![0.0f32; n_bands];
    let mut onset = Vec::with_capacity((samples.len() - frame_size) / hop_size + 1);

    let mut start = 0;
    while start + frame_size <= samples.len() {
        let frame = &samples[start..start + frame_size];

        // Compute magnitude at each band using Goertzel algorithm
        let mut magnitudes = [0.0f32; 32]; // stack alloc, n_bands <= 32
        for (band, &coeff) in band_coeffs.iter().enumerate() {
            let mut s1 = 0.0f32;
            let mut s2 = 0.0f32;
            for (&sample, &win) in frame.iter().zip(window.iter()) {
                let x = if sample.is_finite() {
                    sample * win
                } else {
                    0.0
                };
                let s0 = x + coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            magnitudes[band] = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt();
        }

        // Half-wave rectified spectral flux: sum of positive magnitude increases
        let flux: f32 = (0..n_bands)
            .map(|b| (magnitudes[b] - prev_magnitudes[b]).max(0.0))
            .sum();

        onset.push(flux);
        prev_magnitudes[..n_bands].copy_from_slice(&magnitudes[..n_bands]);
        start += hop_size;
    }

    // Noise floor subtraction
    let mean = onset.iter().sum::<f32>() / onset.len().max(1) as f32;
    for value in &mut onset {
        *value = (*value - mean * 0.25).max(0.0);
    }
    onset
}

fn hanning_window_beat(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| {
            let phase = std::f32::consts::PI * 2.0 * i as f32 / size as f32;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
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
        // Check slower sub-harmonics (3/2 and 4/3 of current lag):
        // If there's support at the slower tempo, boost this candidate.
        for (num, den, weight) in [(3u32, 2u32, 0.25f32), (4, 3, 0.20)] {
            let sub_lag = (lag as u32 * num / den) as usize;
            if sub_lag >= min_lag && sub_lag <= max_lag {
                family_score += base_scores[sub_lag] * weight;
            }
        }

        let bpm = 60.0 * envelope_rate / lag as f32;
        let log_ratio = (bpm / 120.0).log2();
        let octave_prior = (-log_ratio.powi(2) / 8.0).exp();
        let score = family_score * (0.75 + 0.25 * octave_prior);
        candidates.push((lag, score));
    }

    // Sub-harmonic resolution: if the best candidate is fast (>135 BPM) and a
    // candidate at 2/3 or 3/4 of that BPM has good support, prefer the slower one.
    let &(best_lag, best_score) = candidates
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let best_bpm = 60.0 * envelope_rate / best_lag as f32;

    let final_lag = resolve_fast_tempo_subharmonic(
        best_lag,
        best_score,
        best_bpm,
        min_lag,
        max_lag,
        &candidates,
    );

    let final_score = candidates
        .iter()
        .find(|(l, _)| *l == final_lag)
        .map_or(best_score, |(_, s)| *s);
    let second_score = candidates
        .iter()
        .filter(|(lag, _)| lag.abs_diff(final_lag) > 2)
        .map(|(_, score)| *score)
        .fold(0.0_f32, f32::max);

    let period = refined_tempo_period(&interval_histogram, final_lag, min_lag, max_lag);
    let bpm = (60.0 * envelope_rate / period).clamp(min_bpm, max_bpm);
    let periodicity = correlations[final_lag.min(correlations.len() - 1)].clamp(0.0, 1.0);
    let confidence = tempo_confidence(
        final_score,
        second_score,
        periodicity,
        zero_lag_energy,
        onset.len(),
    );

    Some(TempoEstimate { bpm, confidence })
}

fn resolve_fast_tempo_subharmonic(
    best_lag: usize,
    best_score: f32,
    best_bpm: f32,
    min_lag: usize,
    max_lag: usize,
    candidates: &[(usize, f32)],
) -> usize {
    if best_bpm <= 135.0 {
        return best_lag;
    }

    let mut sub_candidate = (best_lag, best_score);
    for (num, den) in [(3usize, 2usize), (4, 3)] {
        let sub_lag = best_lag * num / den;
        if sub_lag < min_lag || sub_lag > max_lag {
            continue;
        }
        if let Some(&(_, sub_score)) = candidates.iter().find(|(lag, _)| *lag == sub_lag)
            && sub_score > best_score * 0.70
            && sub_score > sub_candidate.1 * 0.90
        {
            sub_candidate = (sub_lag, sub_score);
        }
    }
    sub_candidate.0
}

fn refined_tempo_period(
    interval_histogram: &[f32],
    final_lag: usize,
    min_lag: usize,
    max_lag: usize,
) -> f32 {
    let refine_start = final_lag.saturating_sub(1).max(min_lag);
    let refine_end = (final_lag + 1).min(max_lag);
    let refine_weight = interval_histogram[refine_start..=refine_end]
        .iter()
        .sum::<f32>();

    if refine_weight > f32::EPSILON {
        (refine_start..=refine_end)
            .map(|lag| lag as f32 * interval_histogram[lag])
            .sum::<f32>()
            / refine_weight
    } else {
        final_lag as f32
    }
}

fn tempo_confidence(
    final_score: f32,
    second_score: f32,
    periodicity: f32,
    zero_lag_energy: f32,
    onset_len: usize,
) -> f32 {
    let separation = if final_score > f32::EPSILON {
        ((final_score - second_score.max(0.0)) / final_score).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let activity = (zero_lag_energy / onset_len as f32).sqrt();
    let activity_gate = (activity * 100.0).clamp(0.0, 1.0);

    (periodicity * (0.75 + 0.25 * separation) * activity_gate).clamp(0.0, 1.0)
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
    let period = 60.0 * envelope_rate / bpm;
    if period < 1.0 || onset.is_empty() {
        return Vec::new();
    }

    // Dynamic programming beat tracking (Ellis 2007):
    // For each frame, find the optimal predecessor beat that maximizes
    // onset strength while penalizing deviations from expected spacing.
    let period_frames = period.round() as usize;
    let search_window = (period * 0.25).round() as usize; // allow 25% deviation
    let penalty_width = period * 0.5; // Gaussian penalty sigma

    // Score function: onset strength at each frame (already noise-floor subtracted)
    // Cumulative score: best path ending at each frame
    let n = onset.len();
    let mut cumulative_score = vec![0.0f32; n];
    let mut predecessor = vec![0usize; n];

    // Initialize: first beat can be anywhere in the first two periods
    let init_range = (period_frames * 2).min(n);
    for i in 0..init_range {
        cumulative_score[i] = onset[i];
        predecessor[i] = i; // self = start of chain
    }

    // Fill DP table
    for i in period_frames.saturating_sub(search_window)..n {
        let search_start = i.saturating_sub(period_frames + search_window);
        let search_end = i
            .saturating_sub(period_frames.saturating_sub(search_window))
            .min(i);

        let mut best_prev_score = f32::NEG_INFINITY;
        let mut best_prev = search_start;

        for (j, previous_score) in cumulative_score
            .iter()
            .enumerate()
            .take(search_end)
            .skip(search_start)
        {
            let distance = i as f32 - j as f32;
            let deviation = distance - period;
            let penalty = -(deviation * deviation) / (2.0 * penalty_width * penalty_width);
            let score = previous_score + penalty.exp() * 0.5;
            if score > best_prev_score {
                best_prev_score = score;
                best_prev = j;
            }
        }

        let candidate = onset[i] + best_prev_score;
        if candidate > cumulative_score[i] {
            cumulative_score[i] = candidate;
            predecessor[i] = best_prev;
        }
    }

    // Backtrace: find the best ending beat in the last period
    let trace_start = n.saturating_sub(period_frames * 2);
    let best_end = (trace_start..n)
        .max_by(|&a, &b| cumulative_score[a].total_cmp(&cumulative_score[b]))
        .unwrap_or(n - 1);

    let mut beat_frames = Vec::new();
    let mut current = best_end;
    loop {
        beat_frames.push(current);
        let prev = predecessor[current];
        if prev == current || prev >= current {
            break;
        }
        current = prev;
    }
    beat_frames.reverse();

    // Assign downbeats
    let downbeat_phase = find_downbeat_phase(onset, &beat_frames);

    beat_frames
        .iter()
        .enumerate()
        .map(|(beat_index, &frame)| {
            let time_seconds = frame as f64 * hop_size as f64 / f64::from(sample_rate);
            BeatPosition {
                time_seconds,
                confidence,
                position_in_bar: ((beat_index + 4 - downbeat_phase) % 4) as u8 + 1,
            }
        })
        .take_while(|beat| beat.time_seconds <= duration)
        .collect()
}

/// Find the phase offset (0-3) that aligns beat index 0 with the strongest
/// downbeat pattern. Looks at onset energy every 4th beat for each candidate phase.
fn find_downbeat_phase(onset: &[f32], beat_frames: &[usize]) -> usize {
    if beat_frames.len() < 4 {
        return 0;
    }
    let mut best_phase = 0;
    let mut best_energy = f32::NEG_INFINITY;
    for phase in 0..4 {
        let energy: f32 = beat_frames
            .iter()
            .skip(phase)
            .step_by(4)
            .filter_map(|&frame| onset.get(frame))
            .sum();
        if energy > best_energy {
            best_energy = energy;
            best_phase = phase;
        }
    }
    best_phase
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

    fn regular_beats(bpm: f32, count: usize) -> Vec<BeatPosition> {
        let interval = 60.0 / f64::from(bpm);
        (0..count)
            .map(|idx| BeatPosition {
                time_seconds: idx as f64 * interval,
                confidence: 1.0,
                position_in_bar: (idx % 4 + 1) as u8,
            })
            .collect()
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
        assert!(result.multi_tempo, "{result:?}");
        let alternate = result.alternate_bpm.expect("alternate bpm");
        assert!(
            (alternate - 90.0).abs() < 5.0 || (alternate - 120.0).abs() < 5.0,
            "alternate {alternate}"
        );
        assert!(result.alternate_coverage >= 0.25);
    }

    #[test]
    fn alternate_respects_coverage_threshold() {
        let mut samples = click_track(120.0, 24.0, 44_100);
        samples.extend(click_track(90.0, 24.0, 44_100));
        let config = BeatConfig {
            alternate_coverage_threshold: 0.75,
            ..BeatConfig::default()
        };
        let result = analyze(&samples, 44_100, config);
        assert!(!result.multi_tempo);
        assert_eq!(result.alternate_bpm, None);
        assert_eq!(result.alternate_coverage, 0.0);
    }

    #[test]
    fn single_tempo_track_has_no_alternate() {
        let samples = click_track(120.0, 20.0, 44_100);
        let result = analyze(&samples, 44_100, BeatConfig::default());
        assert!(result.global_bpm.is_some());
        assert!(!result.multi_tempo);
        assert_eq!(result.alternate_bpm, None);
        assert_eq!(result.alternate_coverage, 0.0);
    }

    #[test]
    fn alternate_disabled_when_threshold_zero() {
        let mut samples = click_track(120.0, 24.0, 44_100);
        samples.extend(click_track(90.0, 24.0, 44_100));
        let config = BeatConfig {
            alternate_coverage_threshold: 0.0,
            ..BeatConfig::default()
        };
        let result = analyze(&samples, 44_100, config);
        assert!(!result.multi_tempo);
        assert_eq!(result.alternate_bpm, None);
    }

    #[test]
    fn display_bpm_uses_the_integer_tempo_that_best_fits_beats() {
        let beats = regular_beats(122.0, 64);
        assert_eq!(display_bpm(122.596, &beats), 122.0);
    }

    #[test]
    fn display_bpm_can_choose_the_upper_integer() {
        let beats = regular_beats(123.0, 64);
        assert_eq!(display_bpm(122.596, &beats), 123.0);
    }

    #[test]
    fn resolves_supported_four_three_alias_below_160_bpm() {
        let chosen =
            resolve_fast_tempo_subharmonic(30, 1.0, 144.0, 20, 80, &[(30, 1.0), (40, 0.92)]);

        assert_eq!(chosen, 40);
    }

    #[test]
    fn keeps_midtempo_candidate_without_subharmonic_support() {
        let chosen =
            resolve_fast_tempo_subharmonic(32, 1.0, 135.0, 20, 80, &[(32, 1.0), (42, 0.55)]);

        assert_eq!(chosen, 32);
    }

    #[test]
    fn significant_alternate_tempo_empty_segments() {
        assert_eq!(significant_alternate_tempo(&[], 120.0, 0.04, 0.25), None);
    }

    #[test]
    fn significant_alternate_tempo_single_matching_segment() {
        let segments = vec![TempoSegment {
            start_seconds: 0.0,
            end_seconds: 30.0,
            bpm: 120.0,
            confidence: 0.8,
        }];
        assert_eq!(
            significant_alternate_tempo(&segments, 120.0, 0.04, 0.25),
            None
        );
    }

    #[test]
    fn significant_alternate_tempo_below_threshold() {
        let segments = vec![
            TempoSegment {
                start_seconds: 0.0,
                end_seconds: 40.0,
                bpm: 120.0,
                confidence: 0.8,
            },
            TempoSegment {
                start_seconds: 40.0,
                end_seconds: 48.0,
                bpm: 90.0,
                confidence: 0.7,
            },
        ];
        // 8/48 = 16.7%, below 25% threshold
        assert_eq!(
            significant_alternate_tempo(&segments, 120.0, 0.04, 0.25),
            None
        );
    }

    #[test]
    fn significant_alternate_tempo_above_threshold() {
        let segments = vec![
            TempoSegment {
                start_seconds: 0.0,
                end_seconds: 24.0,
                bpm: 120.0,
                confidence: 0.8,
            },
            TempoSegment {
                start_seconds: 24.0,
                end_seconds: 48.0,
                bpm: 90.0,
                confidence: 0.7,
            },
        ];
        let result = significant_alternate_tempo(&segments, 120.0, 0.04, 0.25);
        assert!(result.is_some());
        let (bpm, coverage) = result.unwrap();
        assert!((bpm - 90.0).abs() < 1.0);
        assert!((coverage - 0.5).abs() < 0.01);
    }

    #[test]
    fn rejects_silence() {
        let result = analyze(&vec![0.0; 44_100 * 5], 44_100, BeatConfig::default());
        assert_eq!(result.global_bpm, None);
        assert!(result.beats.is_empty());
        assert!(!result.multi_tempo);
    }
}
