//! Optional ONNX-based beat tracking (feature = "onnx-beat").
//!
//! Supports two model formats:
//! - madmom TCN: input `[1, T, 80]`, output `[1, T]` or `[1, T, 1]`
//! - Beat This! (ISMIR 2024): input `[1, T, 128]`, output `[1, T, 2]`
//!   (channel 0 = beat logits, channel 1 = downbeat logits)
//!
//! The model format is auto-detected from the output shape.

use ort::session::Session;

use crate::spectrogram::{
    log_mel_spectrogram_at_sr, pick_peaks_at_sr, resample_linear, DEFAULT_HOP, DEFAULT_N_FFT,
    DEFAULT_SR,
};
use crate::BeatPosition;

/// Configuration for the ONNX beat tracker.
#[derive(Debug, Clone)]
pub struct OnnxBeatConfig {
    /// Minimum peak prominence in the activation function (0.0-1.0).
    pub peak_threshold: f32,
    /// Number of mel bands. Must match the model's expected input features.
    /// Use 128 for Beat This!, 80 for madmom TCN.
    pub n_mels: usize,
    /// FFT window size.
    pub n_fft: usize,
    /// Hop size in samples (at the model's internal sample rate).
    pub hop_size: usize,
    /// Internal sample rate the model expects (audio is resampled to this).
    pub model_sr: u32,
}

impl OnnxBeatConfig {
    /// Configuration preset for Beat This! (ISMIR 2024) models.
    /// Uses 22050 Hz internally (50 fps with 441-sample hop).
    pub fn beat_this() -> Self {
        Self {
            peak_threshold: 0.3,
            n_mels: 128,
            n_fft: 2048,
            hop_size: 441,
            model_sr: 22_050,
        }
    }

    /// Configuration preset for madmom TCN models.
    pub fn madmom() -> Self {
        Self {
            peak_threshold: 0.3,
            n_mels: 80,
            n_fft: DEFAULT_N_FFT,
            hop_size: DEFAULT_HOP,
            model_sr: DEFAULT_SR,
        }
    }
}

impl Default for OnnxBeatConfig {
    fn default() -> Self {
        Self::beat_this()
    }
}

/// Maximum frames per chunk for the Beat This! transformer model.
/// The model's attention layers have a fixed capacity; exceeding this
/// causes shape errors. 1500 frames ~ 30s at 441-sample hop / 44100 Hz.
const MAX_CHUNK_FRAMES: usize = 1500;

/// Overlap between chunks to avoid boundary artifacts (in frames).
const CHUNK_OVERLAP_FRAMES: usize = 75;

/// Run ONNX beat tracking on mono audio.
///
/// Returns beat positions extracted from the model's activation function.
/// The caller must load the ONNX session (from file or bytes) and pass it in.
/// Long inputs are automatically chunked to fit the model's attention window.
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
    let audio = if sample_rate == config.model_sr {
        samples
    } else {
        resampled = resample_linear(samples, sample_rate, config.model_sr);
        &resampled
    };

    let spectrogram = log_mel_spectrogram_at_sr(audio, config.n_fft, config.hop_size, config.n_mels, config.model_sr);
    let n_frames = if audio.len() >= config.n_fft {
        (audio.len() - config.n_fft) / config.hop_size + 1
    } else {
        return Ok(Vec::new());
    };
    let n_mels = config.n_mels;

    let activation = if n_frames <= MAX_CHUNK_FRAMES {
        run_inference(session, &spectrogram, n_frames, n_mels)?
    } else {
        run_chunked_inference(session, &spectrogram, n_frames, n_mels)?
    };

    let beats = pick_peaks_at_sr(&activation, config.peak_threshold, config.hop_size, config.model_sr);
    Ok(beats)
}

/// Process a long spectrogram in overlapping chunks, stitching activations together.
fn run_chunked_inference(
    session: &mut Session,
    spectrogram: &[f32],
    n_frames: usize,
    n_features: usize,
) -> Result<Vec<f32>, String> {
    let step = MAX_CHUNK_FRAMES - CHUNK_OVERLAP_FRAMES;
    let mut full_activation = vec![0.0f32; n_frames];
    let mut start = 0;

    while start < n_frames {
        let end = (start + MAX_CHUNK_FRAMES).min(n_frames);
        let chunk_frames = end - start;

        let chunk_start_byte = start * n_features;
        let chunk_end_byte = end * n_features;
        let chunk_data = &spectrogram[chunk_start_byte..chunk_end_byte];

        let chunk_activation = run_inference(session, chunk_data, chunk_frames, n_features)?;

        // Determine which portion of this chunk's activation to keep.
        // Skip the overlap region at the start (except for the first chunk).
        let keep_start = if start == 0 { 0 } else { CHUNK_OVERLAP_FRAMES / 2 };
        // Skip the overlap region at the end (except for the last chunk).
        let keep_end = if end == n_frames {
            chunk_frames
        } else {
            chunk_frames - CHUNK_OVERLAP_FRAMES / 2
        };

        let dest_start = start + keep_start;
        for (i, &val) in chunk_activation[keep_start..keep_end].iter().enumerate() {
            full_activation[dest_start + i] = val;
        }

        if end == n_frames {
            break;
        }
        start += step;
    }

    Ok(full_activation)
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

    // Auto-detect output format from tensor length:
    // len == n_frames * 2: Beat This! [1, T, 2] (take every other = beat channel, apply sigmoid)
    // len == n_frames: flat [1, T] or [T] activation (madmom style)
    if activation.len() == n_frames * 2 {
        // Beat This! format: interleaved [beat0, downbeat0, beat1, downbeat1, ...]
        Ok(activation.chunks(2).map(|pair| sigmoid(pair[0])).collect())
    } else if activation.len() == n_frames {
        Ok(activation)
    } else {
        // Unknown format, try to use as-is
        Ok(activation)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
