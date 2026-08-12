//! Smoke test: load the Beat This! ONNX model and run inference on a click track.
//!
//! Usage: `cargo run -p beat-salad --features onnx-beat --example onnx_smoke -- models/beat_this.onnx`

use std::env;

use beat_salad::onnx_beat::{OnnxBeatConfig, track_beats};

fn main() {
    let model_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "models/beat_this.onnx".to_string());

    println!("Loading model from: {model_path}");
    let mut session = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(&model_path)
        .unwrap();

    // Generate a 10-second click track at 120 BPM (kick every 0.5s)
    let sr = 44_100u32;
    let duration_samples = sr as usize * 10;
    let mut samples = vec![0.0f32; duration_samples];
    let beat_interval = sr as f32 * 0.5; // 120 BPM
    let mut pos = 0.0f32;
    while (pos as usize) < duration_samples {
        let idx = pos as usize;
        // Short burst of energy as a "click"
        for i in 0..200.min(duration_samples - idx) {
            samples[idx + i] = (i as f32 * 0.05).sin() * (-(i as f32) / 50.0).exp();
        }
        pos += beat_interval;
    }

    println!("Running inference on 10s click track (120 BPM)...");
    let config = OnnxBeatConfig::beat_this();
    match track_beats(&mut session, &samples, sr, &config) {
        Ok(beats) => {
            println!("Detected {} beats:", beats.len());
            for (i, beat) in beats.iter().take(10).enumerate() {
                println!(
                    "  beat {i:2}: t={:.3}s confidence={:.3} bar_pos={}",
                    beat.time_seconds, beat.confidence, beat.position_in_bar
                );
            }
            if beats.len() > 10 {
                println!("  ... ({} more)", beats.len() - 10);
            }
            let expected = 20; // 10s * 2 beats/s at 120 BPM
            println!(
                "\nExpected ~{expected} beats, got {}. {}",
                beats.len(),
                if beats.len().abs_diff(expected) < 5 {
                    "PASS"
                } else {
                    "MISMATCH (may be OK depending on model)"
                }
            );
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
