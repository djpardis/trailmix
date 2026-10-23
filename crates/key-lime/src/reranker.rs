//! Compact transformer that reranks the 24 DSP key scores.
//!
//! The model reads the first [`RERANKER_MAX_FRAMES`] frame-level chroma vectors
//! and the 24 template scores from [`crate::classify_key_scores`], and returns
//! 24 logits in the same `tonic + 12 * mode` layout. The forward pass is plain
//! Rust over 21,528 embedded float32 weights, so it requires no separate
//! inference framework.
//!
//! The architecture and weight order must match
//! `scripts/export_key_reranker_weights.py`. Weights are the labels-only
//! `seed 1729` checkpoint described in `THIRD_PARTY.md`.

use std::sync::OnceLock;

use crate::ChromaFrame;

/// Number of leading chroma frames the reranker reads. Shorter inputs are
/// zero-padded and longer inputs are truncated, matching training.
pub const RERANKER_MAX_FRAMES: usize = 256;

/// SHA-256 of the embedded weight file.
pub const RERANKER_WEIGHTS_SHA256: &str =
    "3ff726768198d2bd18e2bf75b6b64310ff7982c796c411e378ee9766ec3a066b";

const PITCH_CLASSES: usize = 12;
const KEYS: usize = 24;
const MODEL_DIM: usize = 36;
const CONV_CHANNELS: usize = MODEL_DIM / PITCH_CLASSES;
const CONV_KERNEL: usize = 3;
const HEADS: usize = 4;
const HEAD_DIM: usize = MODEL_DIM / HEADS;
const FEED_FORWARD_DIM: usize = 72;
const LAYER_NORM_EPSILON: f32 = 1.0e-5;
const PARAMETER_COUNT: usize = 21_528;

static WEIGHT_BYTES: &[u8] = include_bytes!("../models/key-reranker-v1.f32");
const _: () = assert!(WEIGHT_BYTES.len() == PARAMETER_COUNT * 4);

struct Weights {
    conv_weight: Vec<f32>,
    conv_bias: Vec<f32>,
    position: Vec<f32>,
    norm1_weight: Vec<f32>,
    norm1_bias: Vec<f32>,
    in_projection_weight: Vec<f32>,
    in_projection_bias: Vec<f32>,
    out_projection_weight: Vec<f32>,
    out_projection_bias: Vec<f32>,
    norm2_weight: Vec<f32>,
    norm2_bias: Vec<f32>,
    linear1_weight: Vec<f32>,
    linear1_bias: Vec<f32>,
    linear2_weight: Vec<f32>,
    linear2_bias: Vec<f32>,
    norm_weight: Vec<f32>,
    norm_bias: Vec<f32>,
    output_weight: Vec<f32>,
    output_bias: Vec<f32>,
    prior_weight: Vec<f32>,
}

fn weights() -> &'static Weights {
    static WEIGHTS: OnceLock<Weights> = OnceLock::new();
    WEIGHTS.get_or_init(|| {
        let mut values = WEIGHT_BYTES
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let mut take = |count: usize| values.by_ref().take(count).collect::<Vec<f32>>();
        Weights {
            conv_weight: take(CONV_CHANNELS * CONV_KERNEL),
            conv_bias: take(CONV_CHANNELS),
            position: take(RERANKER_MAX_FRAMES * MODEL_DIM),
            norm1_weight: take(MODEL_DIM),
            norm1_bias: take(MODEL_DIM),
            in_projection_weight: take(3 * MODEL_DIM * MODEL_DIM),
            in_projection_bias: take(3 * MODEL_DIM),
            out_projection_weight: take(MODEL_DIM * MODEL_DIM),
            out_projection_bias: take(MODEL_DIM),
            norm2_weight: take(MODEL_DIM),
            norm2_bias: take(MODEL_DIM),
            linear1_weight: take(FEED_FORWARD_DIM * MODEL_DIM),
            linear1_bias: take(FEED_FORWARD_DIM),
            linear2_weight: take(MODEL_DIM * FEED_FORWARD_DIM),
            linear2_bias: take(MODEL_DIM),
            norm_weight: take(MODEL_DIM),
            norm_bias: take(MODEL_DIM),
            output_weight: take(KEYS * MODEL_DIM),
            output_bias: take(KEYS),
            prior_weight: take(KEYS * KEYS),
        }
    })
}

/// Rerank the 24 DSP key scores with the frame-level chroma sequence.
///
/// `frames` is the sequence returned by [`crate::extract_chroma_frames`] and
/// `dsp_scores` is the output of [`crate::classify_key_scores`] for the global
/// chroma. Each frame is L1-normalized before inference. The returned logits
/// use the `tonic + 12 * mode` layout, and the predicted key is the argmax.
#[must_use]
pub fn rerank_key_scores(frames: &[ChromaFrame], dsp_scores: &[f32; KEYS]) -> [f32; KEYS] {
    let inputs: Vec<[f32; PITCH_CLASSES]> = frames
        .iter()
        .take(RERANKER_MAX_FRAMES)
        .map(|frame| l1_normalized(&frame.values))
        .collect();
    forward(&inputs, dsp_scores)
}

fn l1_normalized(values: &[f32; PITCH_CLASSES]) -> [f32; PITCH_CLASSES] {
    let total = values.iter().map(|value| value.abs()).sum::<f32>();
    if total <= 0.0 || !total.is_finite() {
        return [0.0; PITCH_CLASSES];
    }
    values.map(|value| value / total)
}

fn forward(inputs: &[[f32; PITCH_CLASSES]], dsp_scores: &[f32; KEYS]) -> [f32; KEYS] {
    let weights = weights();
    let zero_frame = [0.0_f32; PITCH_CLASSES];

    let mut hidden = vec![[0.0_f32; MODEL_DIM]; RERANKER_MAX_FRAMES];
    for (time, row) in hidden.iter_mut().enumerate() {
        let frame = inputs.get(time).unwrap_or(&zero_frame);
        for channel in 0..CONV_CHANNELS {
            for pitch in 0..PITCH_CLASSES {
                let mut value = weights.conv_bias[channel];
                for tap in 0..CONV_KERNEL {
                    let source = (pitch + tap + PITCH_CLASSES - CONV_KERNEL / 2) % PITCH_CLASSES;
                    value += weights.conv_weight[channel * CONV_KERNEL + tap] * frame[source];
                }
                row[channel * PITCH_CLASSES + pitch] = value;
            }
        }
        let position = &weights.position[time * MODEL_DIM..(time + 1) * MODEL_DIM];
        for (value, offset) in row.iter_mut().zip(position) {
            *value += offset;
        }
    }

    let normalized: Vec<[f32; MODEL_DIM]> = hidden
        .iter()
        .map(|row| layer_norm(row, &weights.norm1_weight, &weights.norm1_bias))
        .collect();
    let projected: Vec<Vec<f32>> = normalized
        .iter()
        .map(|row| {
            linear(
                row,
                &weights.in_projection_weight,
                &weights.in_projection_bias,
            )
        })
        .collect();
    let attention = self_attention(&projected);
    for (row, context) in hidden.iter_mut().zip(&attention) {
        let output = linear(
            context,
            &weights.out_projection_weight,
            &weights.out_projection_bias,
        );
        for (value, delta) in row.iter_mut().zip(output) {
            *value += delta;
        }
    }

    for row in &mut hidden {
        let normalized = layer_norm(row, &weights.norm2_weight, &weights.norm2_bias);
        let mut expanded = linear(&normalized, &weights.linear1_weight, &weights.linear1_bias);
        for value in &mut expanded {
            *value = value.max(0.0);
        }
        let output = linear(&expanded, &weights.linear2_weight, &weights.linear2_bias);
        for (value, delta) in row.iter_mut().zip(output) {
            *value += delta;
        }
    }

    let mut pooled = [0.0_f32; MODEL_DIM];
    for row in &hidden {
        for (sum, value) in pooled.iter_mut().zip(row) {
            *sum += value;
        }
    }
    for value in &mut pooled {
        *value /= RERANKER_MAX_FRAMES as f32;
    }
    let pooled = layer_norm(&pooled, &weights.norm_weight, &weights.norm_bias);

    let sequence_logits = linear(&pooled, &weights.output_weight, &weights.output_bias);
    let mut logits = [0.0_f32; KEYS];
    for (key, logit) in logits.iter_mut().enumerate() {
        let prior_row = &weights.prior_weight[key * KEYS..(key + 1) * KEYS];
        let prior = prior_row
            .iter()
            .zip(dsp_scores)
            .map(|(weight, score)| weight * score)
            .sum::<f32>();
        *logit = sequence_logits[key] + prior;
    }
    logits
}

fn self_attention(projected: &[Vec<f32>]) -> Vec<[f32; MODEL_DIM]> {
    let scale = 1.0 / (HEAD_DIM as f32).sqrt();
    let mut context = vec![[0.0_f32; MODEL_DIM]; projected.len()];
    let mut scores = vec![0.0_f32; projected.len()];
    for head in 0..HEADS {
        let query_offset = head * HEAD_DIM;
        let key_offset = MODEL_DIM + head * HEAD_DIM;
        let value_offset = 2 * MODEL_DIM + head * HEAD_DIM;
        for (query_row, output) in projected.iter().zip(context.iter_mut()) {
            let query = &query_row[query_offset..query_offset + HEAD_DIM];
            let mut maximum = f32::NEG_INFINITY;
            for (score, key_row) in scores.iter_mut().zip(projected) {
                let key = &key_row[key_offset..key_offset + HEAD_DIM];
                *score = dot(query, key) * scale;
                maximum = maximum.max(*score);
            }
            let mut total = 0.0;
            for score in &mut scores {
                *score = (*score - maximum).exp();
                total += *score;
            }
            for (score, value_row) in scores.iter().zip(projected) {
                let weight = score / total;
                let value = &value_row[value_offset..value_offset + HEAD_DIM];
                for (target, source) in output[query_offset..query_offset + HEAD_DIM]
                    .iter_mut()
                    .zip(value)
                {
                    *target += weight * source;
                }
            }
        }
    }
    context
}

fn linear(input: &[f32], weight: &[f32], bias: &[f32]) -> Vec<f32> {
    weight
        .chunks_exact(input.len())
        .zip(bias)
        .map(|(row, bias)| dot(row, input) + bias)
        .collect()
}

fn layer_norm(input: &[f32; MODEL_DIM], weight: &[f32], bias: &[f32]) -> [f32; MODEL_DIM] {
    let mean = input.iter().sum::<f32>() / MODEL_DIM as f32;
    let variance = input
        .iter()
        .map(|value| (value - mean) * (value - mean))
        .sum::<f32>()
        / MODEL_DIM as f32;
    let inverse = 1.0 / (variance + LAYER_NORM_EPSILON).sqrt();
    let mut output = [0.0_f32; MODEL_DIM];
    for (index, value) in output.iter_mut().enumerate() {
        *value = (input[index] - mean) * inverse * weight[index] + bias[index];
    }
    output
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u32);

    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / 16_777_216.0
        }
    }

    fn case_inputs(seed: u32, frame_count: usize) -> (Vec<ChromaFrame>, [f32; KEYS]) {
        let mut generator = Lcg(seed);
        let frames = (0..frame_count)
            .map(|index| ChromaFrame {
                center_seconds: index as f64,
                values: std::array::from_fn(|_| generator.next()),
            })
            .collect();
        let prior = std::array::from_fn(|_| 2.0 * generator.next() - 1.0);
        (frames, prior)
    }

    #[test]
    fn matches_onnx_reference_logits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/key-reranker-reference.json"
        ))
        .expect("fixture");
        let cases = fixture["cases"].as_array().expect("cases");
        assert_eq!(cases.len(), 4);
        for case in cases {
            let seed = u32::try_from(case["seed"].as_u64().expect("seed")).expect("seed");
            let frame_count =
                usize::try_from(case["frames"].as_u64().expect("frames")).expect("frames");
            let (frames, prior) = case_inputs(seed, frame_count);
            let logits = rerank_key_scores(&frames, &prior);
            for (index, expected) in case["logits"]
                .as_array()
                .expect("logits")
                .iter()
                .enumerate()
            {
                let expected = expected.as_f64().expect("logit") as f32;
                assert!(
                    (logits[index] - expected).abs() < 1.0e-4,
                    "seed {seed} key {index}: {} versus {expected}",
                    logits[index]
                );
            }
        }
    }

    #[test]
    fn handles_empty_and_silent_frames() {
        let silent = vec![
            ChromaFrame {
                center_seconds: 0.0,
                values: [0.0; PITCH_CLASSES],
            };
            3
        ];
        let empty = rerank_key_scores(&[], &[0.0; KEYS]);
        let padded = rerank_key_scores(&silent, &[0.0; KEYS]);
        assert!(empty.iter().all(|value| value.is_finite()));
        assert_eq!(empty, padded);
    }
}
