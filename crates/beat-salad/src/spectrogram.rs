//! Log-mel spectrogram and DSP utilities for beat tracking preprocessing.

#![allow(dead_code)]

use crate::BeatPosition;

pub(crate) const DEFAULT_HOP: usize = 441;
pub(crate) const DEFAULT_N_FFT: usize = 2048;
pub(crate) const DEFAULT_N_MELS: usize = 80;
pub(crate) const DEFAULT_SR: u32 = 44_100;

/// Compute a log-mel spectrogram from mono PCM (assumed 44100 Hz).
#[must_use]
pub fn log_mel_spectrogram(
    samples: &[f32],
    n_fft: usize,
    hop_size: usize,
    n_mels: usize,
) -> Vec<f32> {
    log_mel_spectrogram_at_sr(samples, n_fft, hop_size, n_mels, DEFAULT_SR)
}

/// Compute a log-mel spectrogram at an arbitrary sample rate.
#[must_use]
pub fn log_mel_spectrogram_at_sr(
    samples: &[f32],
    n_fft: usize,
    hop_size: usize,
    n_mels: usize,
    sample_rate: u32,
) -> Vec<f32> {
    let n_frames = if samples.len() >= n_fft {
        (samples.len() - n_fft) / hop_size + 1
    } else {
        return Vec::new();
    };
    let n_bins = n_fft / 2 + 1;

    let mel_filterbank = mel_filters(n_mels, n_bins, sample_rate);
    let window = hann_window(n_fft);

    let mut spectrogram = vec![0.0f32; n_frames * n_mels];

    for frame_idx in 0..n_frames {
        let start = frame_idx * hop_size;
        let end = start + n_fft;

        let mut power_spectrum = vec![0.0f32; n_bins];
        compute_power_spectrum(&samples[start..end], &window, &mut power_spectrum);

        let row_start = frame_idx * n_mels;
        for (mel_idx, mel_row) in mel_filterbank.iter().enumerate() {
            let energy: f32 = power_spectrum
                .iter()
                .zip(mel_row.iter())
                .map(|(p, w)| p * w)
                .sum();
            spectrogram[row_start + mel_idx] = (energy.max(1e-10)).ln();
        }
    }

    spectrogram
}

/// Returns `(n_frames, n_mels)` for a given sample count and config.
#[must_use]
pub fn spectrogram_shape(
    sample_count: usize,
    n_fft: usize,
    hop_size: usize,
    n_mels: usize,
) -> (usize, usize) {
    let n_frames = if sample_count >= n_fft {
        (sample_count - n_fft) / hop_size + 1
    } else {
        0
    };
    (n_frames, n_mels)
}

/// Pick peaks from a beat activation function using adaptive thresholding.
#[must_use]
pub fn pick_peaks(activation: &[f32], threshold: f32, hop_size: usize) -> Vec<BeatPosition> {
    pick_peaks_at_sr(activation, threshold, hop_size, DEFAULT_SR)
}

/// Pick peaks from a beat activation function at a given sample rate.
#[must_use]
pub fn pick_peaks_at_sr(activation: &[f32], threshold: f32, hop_size: usize, sample_rate: u32) -> Vec<BeatPosition> {
    if activation.is_empty() {
        return Vec::new();
    }

    let frame_rate = sample_rate as f32 / hop_size as f32;
    let mut beats = Vec::new();
    let window = 5;
    let min_inter_beat_frames = (frame_rate * 0.2) as usize;
    let mut last_beat_frame: Option<usize> = None;

    for i in window..activation.len().saturating_sub(window) {
        if activation[i] < threshold {
            continue;
        }

        let is_peak = (i.saturating_sub(window)..i)
            .chain(i + 1..=(i + window).min(activation.len() - 1))
            .all(|j| activation[i] >= activation[j]);

        if !is_peak {
            continue;
        }

        if let Some(last) = last_beat_frame {
            if i - last < min_inter_beat_frames {
                continue;
            }
        }

        last_beat_frame = Some(i);
        let time_seconds = i as f64 * hop_size as f64 / f64::from(sample_rate);
        beats.push(BeatPosition {
            time_seconds,
            confidence: activation[i],
            position_in_bar: 0,
        });
    }

    assign_downbeats(&mut beats, activation, hop_size, sample_rate);
    beats
}

/// Assign `position_in_bar` (1-4) by finding the 4-beat grouping that maximizes
/// activation energy on the downbeat positions.
fn assign_downbeats(beats: &mut [BeatPosition], activation: &[f32], hop_size: usize, sample_rate: u32) {
    if beats.len() < 4 {
        for (i, beat) in beats.iter_mut().enumerate() {
            beat.position_in_bar = (i % 4) as u8 + 1;
        }
        return;
    }

    let mut best_phase = 0;
    let mut best_energy = f32::NEG_INFINITY;
    for phase in 0..4 {
        let energy: f32 = beats
            .iter()
            .skip(phase)
            .step_by(4)
            .map(|b| {
                let frame = (b.time_seconds * f64::from(sample_rate) / hop_size as f64) as usize;
                activation.get(frame).copied().unwrap_or(0.0)
            })
            .sum();
        if energy > best_energy {
            best_energy = energy;
            best_phase = phase;
        }
    }

    for (i, beat) in beats.iter_mut().enumerate() {
        beat.position_in_bar = ((i + 4 - best_phase) % 4) as u8 + 1;
    }
}

// ——— DSP helpers ———

fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let phase = std::f32::consts::PI * 2.0 * i as f32 / n as f32;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

fn compute_power_spectrum(frame: &[f32], window: &[f32], out: &mut [f32]) {
    let n_fft = frame.len();
    let n_bins = n_fft / 2 + 1;
    debug_assert!(out.len() >= n_bins);
    debug_assert!(window.len() == n_fft);

    for (bin, power) in out.iter_mut().enumerate().take(n_bins) {
        let freq = std::f32::consts::PI * 2.0 * bin as f32 / n_fft as f32;
        let mut real = 0.0f32;
        let mut imag = 0.0f32;
        for (k, (&sample, &win)) in frame.iter().zip(window.iter()).enumerate() {
            let x = sample * win;
            let phase = freq * k as f32;
            real += x * phase.cos();
            imag -= x * phase.sin();
        }
        *power = (real * real + imag * imag) / n_fft as f32;
    }
}

/// Build triangular mel filterbank (`n_mels` x `n_bins`).
fn mel_filters(n_mels: usize, n_bins: usize, sample_rate: u32) -> Vec<Vec<f32>> {
    let fmax = sample_rate as f32 / 2.0;
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(fmax);

    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
        .collect();

    let freq_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    let bin_freqs: Vec<f32> = (0..n_bins)
        .map(|b| b as f32 * sample_rate as f32 / ((n_bins - 1) * 2) as f32)
        .collect();

    let mut filters = vec![vec![0.0f32; n_bins]; n_mels];
    for (mel_idx, filter) in filters.iter_mut().enumerate() {
        let f_left = freq_points[mel_idx];
        let f_center = freq_points[mel_idx + 1];
        let f_right = freq_points[mel_idx + 2];

        for (bin_idx, &freq) in bin_freqs.iter().enumerate() {
            if freq >= f_left && freq <= f_center {
                filter[bin_idx] = (freq - f_left) / (f_center - f_left).max(f32::EPSILON);
            } else if freq > f_center && freq <= f_right {
                filter[bin_idx] = (f_right - freq) / (f_right - f_center).max(f32::EPSILON);
            }
        }
    }
    filters
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

/// Linear resampling (simple, adequate for the spectrogram input).
pub(crate) fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from_rate) / f64::from(to_rate);
    let out_len = (samples.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let sample = if idx + 1 < samples.len() {
            f64::from(samples[idx]) * (1.0 - frac) + f64::from(samples[idx + 1]) * frac
        } else {
            f64::from(samples[idx.min(samples.len() - 1)])
        };
        output.push(sample as f32);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_filterbank_dimensions() {
        let filters = mel_filters(80, 1025, 44_100);
        assert_eq!(filters.len(), 80);
        assert_eq!(filters[0].len(), 1025);
    }

    #[test]
    fn log_mel_spectrogram_shape() {
        let sr = 44_100;
        let duration_seconds = 1.0;
        let samples = vec![0.0f32; (sr as f32 * duration_seconds) as usize];
        let spec = log_mel_spectrogram(&samples, 2048, 441, 80);
        let (expected_frames, expected_mels) = spectrogram_shape(samples.len(), 2048, 441, 80);
        assert_eq!(spec.len(), expected_frames * expected_mels);
    }

    #[test]
    fn peak_picking_basic() {
        let mut activation = vec![0.0f32; 100];
        activation[20] = 0.8;
        activation[50] = 0.9;
        activation[80] = 0.7;
        let beats = pick_peaks(&activation, 0.3, 441);
        assert_eq!(beats.len(), 3);
        assert!(beats[0].time_seconds < beats[1].time_seconds);
    }

    #[test]
    fn resample_preserves_duration() {
        let sr = 48_000;
        let samples = vec![0.5f32; sr as usize];
        let resampled = resample_linear(&samples, sr, DEFAULT_SR);
        let expected_len = (samples.len() as f64 * f64::from(DEFAULT_SR) / f64::from(sr)).ceil();
        assert!((resampled.len() as f64 - expected_len).abs() < 2.0);
    }
}
