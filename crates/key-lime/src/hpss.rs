//! Harmonic-Percussive Source Separation (HPSS) via median filtering.
//!
//! Fitzgerald (2010): separates a spectrogram into harmonic (horizontally smooth)
//! and percussive (vertically smooth) components. We only need the harmonic
//! time-domain signal for chroma extraction.

/// Apply HPSS and return the harmonic component as a new sample buffer.
/// Uses STFT with the given frame/hop sizes, median-filters the magnitude
/// spectrogram in both directions, then soft-masks and resynthesizes.
pub fn harmonic_component(samples: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    let n_bins = frame_size / 2 + 1;
    let n_frames = if samples.len() >= frame_size {
        (samples.len() - frame_size) / hop_size + 1
    } else {
        return samples.to_vec();
    };

    let window = hann_window(frame_size);

    // Forward STFT: store complex (real, imag) per bin per frame
    let mut stft_real = vec![vec![0.0f32; n_bins]; n_frames];
    let mut stft_imag = vec![vec![0.0f32; n_bins]; n_frames];
    let mut magnitude = vec![vec![0.0f32; n_bins]; n_frames];

    for frame_idx in 0..n_frames {
        let start = frame_idx * hop_size;
        for bin in 0..n_bins {
            let freq = std::f32::consts::PI * 2.0 * bin as f32 / frame_size as f32;
            let mut real = 0.0f32;
            let mut imag = 0.0f32;
            for k in 0..frame_size {
                let x = samples[start + k] * window[k];
                let phase = freq * k as f32;
                real += x * phase.cos();
                imag -= x * phase.sin();
            }
            stft_real[frame_idx][bin] = real;
            stft_imag[frame_idx][bin] = imag;
            magnitude[frame_idx][bin] = (real * real + imag * imag).sqrt();
        }
    }

    // Median filter kernel sizes (in frames/bins)
    let h_kernel = 17; // horizontal: captures sustained tones
    let v_kernel = 17; // vertical: captures transient attacks

    // Harmonic-enhanced spectrogram: horizontal median (across time for each bin)
    let harmonic_mag = median_filter_horizontal(&magnitude, h_kernel, n_frames, n_bins);
    // Percussive-enhanced spectrogram: vertical median (across frequency for each frame)
    let percussive_mag = median_filter_vertical(&magnitude, v_kernel, n_frames, n_bins);

    // Soft Wiener-style mask: H / (H + P + eps)
    let mut output = vec![0.0f32; samples.len()];
    let mut window_sum = vec![0.0f32; samples.len()];
    let eps = 1e-10f32;

    for frame_idx in 0..n_frames {
        let start = frame_idx * hop_size;
        for bin in 0..n_bins {
            let h = harmonic_mag[frame_idx][bin];
            let p = percussive_mag[frame_idx][bin];
            let mask = h / (h + p + eps);
            stft_real[frame_idx][bin] *= mask;
            stft_imag[frame_idx][bin] *= mask;
        }

        // Inverse DFT for this frame
        for k in 0..frame_size {
            let mut sample = 0.0f32;
            for bin in 0..n_bins {
                let freq = std::f32::consts::PI * 2.0 * bin as f32 / frame_size as f32;
                let phase = freq * k as f32;
                let contribution =
                    stft_real[frame_idx][bin] * phase.cos() - stft_imag[frame_idx][bin] * phase.sin();
                if bin == 0 || bin == n_bins - 1 {
                    sample += contribution;
                } else {
                    sample += 2.0 * contribution;
                }
            }
            sample /= frame_size as f32;

            if start + k < output.len() {
                output[start + k] += sample * window[k];
                window_sum[start + k] += window[k] * window[k];
            }
        }
    }

    // Normalize by overlap-add window sum
    for (sample, &w) in output.iter_mut().zip(window_sum.iter()) {
        if w > 1e-8 {
            *sample /= w;
        }
    }

    output
}

fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let phase = std::f32::consts::PI * 2.0 * i as f32 / n as f32;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

/// Horizontal median filter: for each (frame, bin), take the median of
/// `kernel` frames centered on that position along the time axis.
fn median_filter_horizontal(
    mag: &[Vec<f32>],
    kernel: usize,
    n_frames: usize,
    n_bins: usize,
) -> Vec<Vec<f32>> {
    let half = kernel / 2;
    let mut result = vec![vec![0.0f32; n_bins]; n_frames];
    let mut buf = Vec::with_capacity(kernel);

    for bin in 0..n_bins {
        for frame in 0..n_frames {
            buf.clear();
            let start = frame.saturating_sub(half);
            let end = (frame + half + 1).min(n_frames);
            for f in start..end {
                buf.push(mag[f][bin]);
            }
            let mid = buf.len() / 2;
            let (_, median, _) = buf.select_nth_unstable_by(mid, f32::total_cmp);
            result[frame][bin] = *median;
        }
    }
    result
}

/// Vertical median filter: for each (frame, bin), take the median of
/// `kernel` bins centered on that position along the frequency axis.
fn median_filter_vertical(
    mag: &[Vec<f32>],
    kernel: usize,
    n_frames: usize,
    n_bins: usize,
) -> Vec<Vec<f32>> {
    let half = kernel / 2;
    let mut result = vec![vec![0.0f32; n_bins]; n_frames];
    let mut buf = Vec::with_capacity(kernel);

    for frame in 0..n_frames {
        for bin in 0..n_bins {
            buf.clear();
            let start = bin.saturating_sub(half);
            let end = (bin + half + 1).min(n_bins);
            for b in start..end {
                buf.push(mag[frame][b]);
            }
            let mid = buf.len() / 2;
            let (_, median, _) = buf.select_nth_unstable_by(mid, f32::total_cmp);
            result[frame][bin] = *median;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmonic_component_preserves_length() {
        let samples = vec![0.5f32; 44_100];
        let result = harmonic_component(&samples, 2048, 512);
        assert_eq!(result.len(), samples.len());
    }

    #[test]
    fn silence_stays_silent() {
        let samples = vec![0.0f32; 8192];
        let result = harmonic_component(&samples, 1024, 512);
        let max_val = result.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_val < 1e-6, "expected silence, got max {max_val}");
    }

    #[test]
    fn sine_survives_hpss() {
        let sr = 44_100.0f32;
        let freq = 440.0;
        let samples: Vec<f32> = (0..44_100)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect();
        let harmonic = harmonic_component(&samples, 2048, 512);
        let input_energy: f32 = samples.iter().map(|s| s * s).sum();
        let harmonic_energy: f32 = harmonic.iter().map(|s| s * s).sum();
        let ratio = harmonic_energy / input_energy;
        assert!(
            ratio > 0.5,
            "pure sine should be mostly harmonic, got ratio {ratio}"
        );
    }
}
