//! Compare ONNX beat tracking with heuristic beat tracking on real audio files.
//! Reports agreement rate, timing, and per-track beat counts.
//!
//! Usage:
//! `cargo run --release --example onnx_vs_heuristic -p trailmix-bench --features "" -- <model.onnx> <audio_dir> [max_tracks]`

use std::{env, fs, time::Instant};

use beat_salad::onnx_beat::{OnnxBeatConfig, track_beats};
use trailmix::{AnalysisConfig, AudioBuffer};

#[allow(clippy::too_many_lines)]
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: onnx_vs_heuristic <model.onnx> <audio_dir> [max_tracks]");
        std::process::exit(1);
    }

    let model_path = &args[1];
    let audio_dir = &args[2];
    let max_tracks: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20);

    let mut session = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(model_path)
        .unwrap();

    let audio_files: Vec<_> = fs::read_dir(audio_dir)
        .expect("cannot read audio dir")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_str()?;
            if ["mp3", "flac", "wav", "ogg", "m4a"].contains(&ext) {
                Some(path)
            } else {
                None
            }
        })
        .take(max_tracks)
        .collect();

    println!(
        "Comparing ONNX vs heuristic on {} tracks\n",
        audio_files.len()
    );
    println!(
        "{:<40} {:>6} {:>6} {:>8} {:>8} {:>8}",
        "Track", "Heur.", "ONNX", "Agree%", "Heur ms", "ONNX ms"
    );
    println!("{}", "-".repeat(82));

    let config = OnnxBeatConfig::beat_this();
    let tolerance = 0.070; // 70ms tolerance for agreement

    let mut total_heuristic_beats = 0usize;
    let mut total_onnx_beats = 0usize;
    let mut total_agreed = 0usize;
    let mut total_heuristic_ms = 0.0f64;
    let mut total_onnx_ms = 0.0f64;
    let mut track_count = 0usize;

    for path in &audio_files {
        let decoded = match trailmix_codecs::decode_file(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("  skip {}: {e}", path.display());
                continue;
            }
        };

        // Heuristic beats
        let t0 = Instant::now();
        let analysis = trailmix::analyze(
            AudioBuffer {
                samples: &decoded.samples,
                sample_rate: decoded.sample_rate,
            },
            AnalysisConfig::default(),
        );
        let heuristic_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let heuristic_beats = &analysis.beat.beats;

        // ONNX beats
        let t1 = Instant::now();
        let onnx_beats =
            match track_beats(&mut session, &decoded.samples, decoded.sample_rate, &config) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("  skip {} (onnx): {e}", path.display());
                    continue;
                }
            };
        let onnx_ms = t1.elapsed().as_secs_f64() * 1000.0;

        // Measure agreement: for each heuristic beat, is there an ONNX beat within tolerance?
        let agreed = heuristic_beats
            .iter()
            .filter(|hb| {
                onnx_beats
                    .iter()
                    .any(|ob| (ob.time_seconds - hb.time_seconds).abs() < tolerance)
            })
            .count();

        let agreement_pct = if heuristic_beats.is_empty() {
            0.0
        } else {
            100.0 * agreed as f64 / heuristic_beats.len() as f64
        };

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let display_name: String = if name.len() > 38 {
            name[..38].to_string()
        } else {
            name
        };

        println!(
            "{:<40} {:>6} {:>6} {:>7.1}% {:>7.1} {:>7.1}",
            display_name,
            heuristic_beats.len(),
            onnx_beats.len(),
            agreement_pct,
            heuristic_ms,
            onnx_ms,
        );

        total_heuristic_beats += heuristic_beats.len();
        total_onnx_beats += onnx_beats.len();
        total_agreed += agreed;
        total_heuristic_ms += heuristic_ms;
        total_onnx_ms += onnx_ms;
        track_count += 1;
    }

    println!("{}", "-".repeat(82));
    let overall_agreement = if total_heuristic_beats == 0 {
        0.0
    } else {
        100.0 * total_agreed as f64 / total_heuristic_beats as f64
    };
    println!("\nSummary ({track_count} tracks):");
    println!("  Heuristic total beats: {total_heuristic_beats}");
    println!("  ONNX total beats:      {total_onnx_beats}");
    println!("  Agreement:             {overall_agreement:.1}%");
    println!(
        "  Heuristic total time:  {:.1}ms ({:.1}ms/track)",
        total_heuristic_ms,
        total_heuristic_ms / track_count as f64
    );
    println!(
        "  ONNX total time:       {:.1}ms ({:.1}ms/track)",
        total_onnx_ms,
        total_onnx_ms / track_count as f64
    );
}
