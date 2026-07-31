//! Optional ONNX-based beat tracking (feature = "onnx-beat").
//!
//! Expects a model that conforms to the following interface:
//! - Input: `[1, num_frames, num_features]` float32 (log-mel spectrogram)
//! - Output: `[1, num_frames]` or `[1, num_frames, 1]` float32 (beat activation)
//!
//! Compatible with madmom TCN / BeatNet-style models exported to ONNX.

use ort::session::Session;

use crate::BeatPosition;
use crate::spectrogram::{
    DEFAULT_HOP, DEFAULT_N_FFT, DEFAULT_N_MELS, DEFAULT_SR, log_mel_spectrogram, pick_peaks,
    resample_linear, spectrogram_shape,
};

/// Configuration for the ONNX beat tracker.
#[derive(Debug, Clone)]
pub struct OnnxBeatConfig {
    /// Minimum peak prominence in the activation function (0.0-1.0).
    pub peak_threshold: f32,
    /// Number of mel bands. Must match the model's expected input features.
    pub n_mels: usize,
    /// FFT window size.
    pub n_fft: usize,
    /// Hop size in samples (at 44100 Hz).
    pub hop_size: usize,
}

impl Default for OnnxBeatConfig {
    fn default() -> Self {
        Self {
            peak_threshold: 0.3,
            n_mels: DEFAULT_N_MELS,
            n_fft: DEFAULT_N_FFT,
            hop_size: DEFAULT_HOP,
        }
    }
}

/// Run ONNX beat tracking on mono audio.
///
/// Returns beat positions extracted from the model's activation function.
/// The caller must load the ONNX session (from file or bytes) and pass it in.
///
/// # Errors
/// Returns an error string if inference fails.
pub fn track_beats(
    session: &mut Session,
    samples: &[f32],
    sample_rate: u32,
    config: &OnnxBeatConfig,
) -> Result<Vec<BeatPosition>, String> {
    let resampled;
    let audio = if sample_rate == DEFAULT_SR {
        samples
    } else {
        resampled = resample_linear(samples, sample_rate, DEFAULT_SR);
        &resampled
    };

    let spectrogram = log_mel_spectrogram(audio, config.n_fft, config.hop_size, config.n_mels);
    let (n_frames, n_mels) =
        spectrogram_shape(audio.len(), config.n_fft, config.hop_size, config.n_mels);

    let activation = run_inference(session, &spectrogram, n_frames, n_mels)?;
    let beats = pick_peaks(&activation, config.peak_threshold, config.hop_size);
    Ok(beats)
}

fn run_inference(
    session: &mut Session,
    spectrogram: &[f32],
    n_frames: usize,
    n_features: usize,
) -> Result<Vec<f32>, String> {
    let input_value = ort::value::Value::from_array((
        [1, n_frames, n_features],
        spectrogram.to_vec().into_boxed_slice(),
    ))
    .map_err(|e| format!("input error: {e}"))?;

    let outputs = session
        .run(ort::inputs![input_value])
        .map_err(|e| format!("inference error: {e}"))?;

    let (_, activation_slice) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("output extraction error: {e}"))?;

    let activation: Vec<f32> = activation_slice.to_vec();
    if activation.len() == n_frames {
        Ok(activation)
    } else if activation.len() == n_frames * 2 {
        Ok(activation.iter().skip(1).step_by(2).copied().collect())
    } else {
        Ok(activation)
    }
}
