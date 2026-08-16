use std::{
    env,
    error::Error,
    fmt::Write as FmtWrite,
    fs::File,
    io::{BufWriter, Read, Write as IoWrite},
    path::Path,
    process::{Command, ExitCode},
    time::Instant,
};

use rayon::prelude::*;
#[cfg(feature = "cueport-db")]
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use trailmix::{Analysis, AnalysisConfig, AudioBuffer, BeatPosition, Mode, MusicalKey, PitchClass};
use trailmix_manifest::{
    AppleMusicUnderstandingObservation, BeatAnnotation, KeySegmentAnnotation, SeratoObservation,
    TempoSegmentAnnotation, TrackAnnotation,
};

const SAMPLE_RATE: u32 = 44_100;
const DURATION_SECONDS: u32 = 20;

/// Discarded, unmeasured calls that let allocators and caches settle before timing starts.
const WARMUP_RUNS: usize = 1;
/// Measured calls per track. Reporting the median of several runs, rather than one
/// untuned call, keeps a single slow scheduling tick from skewing the result.
const TIMED_RUNS: usize = 5;

#[derive(Serialize)]
struct SyntheticBenchmark {
    version: u32,
    sample_rate: u32,
    duration_seconds: u32,
    warmup_runs: usize,
    timed_runs: usize,
    cases: Vec<SyntheticCase>,
}

#[derive(Serialize)]
struct SyntheticCase {
    expected_bpm: f32,
    detected_bpm: Option<f32>,
    absolute_error: Option<f32>,
    confidence: f32,
    median_elapsed_milliseconds: f64,
}

#[derive(Serialize)]
struct CorpusBenchmark {
    version: u32,
    manifest_version: u32,
    track_count: usize,
    warmup_runs: usize,
    timed_runs: usize,
    summary: CorpusSummary,
    tracks: Vec<TrackResult>,
}

#[derive(Serialize)]
struct CorpusSummary {
    analyzed_tracks: usize,
    failed_tracks: usize,
    bpm_mean_absolute_error: Option<f32>,
    bpm_octave_aware_mean_absolute_error: Option<f32>,
    exact_key_accuracy: Option<f32>,
    mirex_weighted_score: Option<f32>,
    tempo_segment_mean_absolute_error: Option<f32>,
    key_segment_exact_accuracy: Option<f32>,
    beat_f1: Option<f32>,
    beat_precision: Option<f32>,
    beat_recall: Option<f32>,
    multi_tempo_tracks: usize,
    multi_key_tracks: usize,
    serato_bpm_mean_absolute_agreement: Option<f32>,
    serato_bpm_octave_aware_mean_absolute_agreement: Option<f32>,
    serato_exact_key_agreement: Option<f32>,
    apple_music_understanding_bpm_mean_absolute_agreement: Option<f32>,
    apple_music_understanding_bpm_octave_aware_mean_absolute_agreement: Option<f32>,
    apple_music_understanding_exact_key_agreement: Option<f32>,
    mean_decode_milliseconds: Option<f64>,
    mean_analysis_milliseconds: Option<f64>,
    mean_beat_analysis_milliseconds: Option<f64>,
    mean_key_analysis_milliseconds: Option<f64>,
    mean_waveform_analysis_milliseconds: Option<f64>,
}

#[derive(Serialize)]
struct TrackResult {
    id: String,
    split: Option<String>,
    duration_seconds: Option<f64>,
    decode_milliseconds: Option<f64>,
    /// Median of `TIMED_RUNS` calls, after `WARMUP_RUNS` discarded calls. Sum of the
    /// three per-analyzer medians below, so it is comparable across tracks even
    /// though the three analyzers run one after another inside `analyze()`.
    analysis_milliseconds: Option<f64>,
    beat_analysis_milliseconds: Option<f64>,
    key_analysis_milliseconds: Option<f64>,
    waveform_analysis_milliseconds: Option<f64>,
    expected_bpm: Option<f32>,
    detected_bpm: Option<f32>,
    bpm_absolute_error: Option<f32>,
    bpm_octave_aware_absolute_error: Option<f32>,
    expected_key: Option<String>,
    detected_key: Option<String>,
    exact_key_match: Option<bool>,
    mirex_score: Option<f32>,
    tempo_segment_mean_absolute_error: Option<f32>,
    key_segment_exact_accuracy: Option<f32>,
    beat_f1: Option<f32>,
    beat_precision: Option<f32>,
    beat_recall: Option<f32>,
    multi_tempo: bool,
    alternate_bpm: Option<f32>,
    alternate_bpm_coverage: f32,
    multi_key: bool,
    alternate_key: Option<String>,
    alternate_key_coverage: f32,
    serato_bpm: Option<f32>,
    serato_key: Option<String>,
    serato_bpm_absolute_agreement: Option<f32>,
    serato_bpm_octave_aware_absolute_agreement: Option<f32>,
    serato_exact_key_agreement: Option<bool>,
    apple_music_understanding_bpm: Option<f32>,
    apple_music_understanding_key: Option<String>,
    apple_music_understanding_bpm_absolute_agreement: Option<f32>,
    apple_music_understanding_bpm_octave_aware_absolute_agreement: Option<f32>,
    apple_music_understanding_exact_key_agreement: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chroma: Option<[f32; 12]>,
    error: Option<String>,
}

#[derive(Serialize)]
struct CueportSeratoFolderBenchmark {
    version: u32,
    trailmix_git_sha: Option<String>,
    cueport_db: String,
    folder: String,
    reference_tracks: usize,
    analyzed_tracks: usize,
    decode_errors: usize,
    no_bpm_tracks: usize,
    summary: CueportSeratoSummary,
    tracks: Vec<CueportSeratoTrackResult>,
}

#[derive(Serialize)]
struct CueportSeratoSummary {
    bpm_mean_absolute_error: Option<f32>,
    bpm_median_absolute_error: Option<f32>,
    bpm_octave_aware_mean_absolute_error: Option<f32>,
    bpm_within_one: usize,
    bpm_octave_within_one: usize,
    key_exact_accuracy: Option<f32>,
    key_mirex_mean: Option<f32>,
}

#[derive(Serialize)]
struct CueportSeratoTrackResult {
    path: String,
    sha256: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    serato_bpm: Option<f32>,
    serato_key: Option<String>,
    trailmix_global_bpm: Option<f32>,
    trailmix_display_bpm: Option<f32>,
    bpm_absolute_error: Option<f32>,
    bpm_octave_aware_error: Option<f32>,
    trailmix_key: Option<String>,
    exact_key_match: Option<bool>,
    mirex_score: Option<f32>,
    decode_milliseconds: Option<f64>,
    analysis_milliseconds: Option<f64>,
    status: CueportSeratoStatus,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum CueportSeratoStatus {
    Analyzed,
    DecodeError,
    NoBpm,
    MissingFile,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("trailmix-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let mut manifest_path: Option<&str> = None;
    let mut limit: Option<usize> = None;
    let mut parallel = false;
    let mut fast = false;
    let mut onnx_beats_path: Option<&str> = None;
    let mut cueport_db: Option<&str> = None;
    let mut cueport_serato_folder: Option<&str> = None;
    let mut jsonl_path: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                i += 1;
                manifest_path = Some(args.get(i).map(String::as_str).ok_or_else(usage)?);
            }
            "--limit" => {
                i += 1;
                limit = Some(
                    args.get(i)
                        .ok_or("--limit requires a number")?
                        .parse::<usize>()?,
                );
            }
            "--parallel" => parallel = true,
            "--fast" => fast = true,
            "--onnx-beats" => {
                i += 1;
                onnx_beats_path = Some(
                    args.get(i)
                        .map(String::as_str)
                        .ok_or("--onnx-beats requires a model path")?,
                );
            }
            "--cueport-db" => {
                i += 1;
                cueport_db = Some(args.get(i).map(String::as_str).ok_or_else(usage)?);
            }
            "--cueport-serato-folder" => {
                i += 1;
                cueport_serato_folder = Some(args.get(i).map(String::as_str).ok_or_else(usage)?);
            }
            "--jsonl" => {
                i += 1;
                jsonl_path = Some(args.get(i).map(String::as_str).ok_or_else(usage)?);
            }
            _ => {
                return Err(usage().into());
            }
        }
        i += 1;
    }

    let result = if let Some(folder) = cueport_serato_folder {
        let db = cueport_db.ok_or("--cueport-db is required with --cueport-serato-folder")?;
        serde_json::to_string_pretty(&run_cueport_serato_folder(
            Path::new(db),
            Path::new(folder),
            limit,
            jsonl_path.map(Path::new),
        )?)?
    } else {
        match manifest_path {
            None => serde_json::to_string_pretty(&run_synthetic())?,
            Some(path) => serde_json::to_string_pretty(&run_manifest(
                Path::new(path),
                limit,
                parallel,
                fast,
                onnx_beats_path,
            )?)?,
        }
    };
    println!("{result}");
    Ok(())
}

fn usage() -> String {
    "usage: trailmix-bench [--manifest <path>] [--limit N] [--parallel] [--fast] [--onnx-beats <model>] [--cueport-db <cueport.db> --cueport-serato-folder <folder> [--jsonl <path>]]".into()
}

fn run_cueport_serato_folder(
    db_path: &Path,
    folder: &Path,
    limit: Option<usize>,
    jsonl_path: Option<&Path>,
) -> Result<CueportSeratoFolderBenchmark, Box<dyn Error>> {
    #[cfg(not(feature = "cueport-db"))]
    {
        let _ = (db_path, folder, limit, jsonl_path);
        Err("Cueport Serato folder comparison requires --features cueport-db".into())
    }

    #[cfg(feature = "cueport-db")]
    {
        let tracks = cueport_serato_tracks(db_path, folder, limit)?;
        let mut jsonl = if let Some(path) = jsonl_path {
            Some(BufWriter::new(File::create(path)?))
        } else {
            None
        };
        let mut results = Vec::with_capacity(tracks.len());

        for (idx, track) in tracks.iter().enumerate() {
            eprintln!(
                "[{}/{}] {}",
                idx + 1,
                tracks.len(),
                track.title.as_deref().unwrap_or(&track.path)
            );
            let result = analyze_cueport_serato_track(track);
            if let Some(writer) = jsonl.as_mut() {
                serde_json::to_writer(&mut *writer, &result)?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            results.push(result);
        }

        Ok(CueportSeratoFolderBenchmark {
            version: 1,
            trailmix_git_sha: trailmix_git_sha(),
            cueport_db: db_path.display().to_string(),
            folder: folder.display().to_string(),
            reference_tracks: tracks.len(),
            analyzed_tracks: results
                .iter()
                .filter(|track| matches!(track.status, CueportSeratoStatus::Analyzed))
                .count(),
            decode_errors: results
                .iter()
                .filter(|track| matches!(track.status, CueportSeratoStatus::DecodeError))
                .count(),
            no_bpm_tracks: results
                .iter()
                .filter(|track| matches!(track.status, CueportSeratoStatus::NoBpm))
                .count(),
            summary: cueport_serato_summary(&results),
            tracks: results,
        })
    }
}

struct CueportSeratoDbTrack {
    path: String,
    title: Option<String>,
    artist: Option<String>,
    bpm: Option<f32>,
    key: Option<String>,
}

#[cfg(feature = "cueport-db")]
fn cueport_serato_tracks(
    db_path: &Path,
    folder: &Path,
    limit: Option<usize>,
) -> Result<Vec<CueportSeratoDbTrack>, Box<dyn Error>> {
    let conn = Connection::open(db_path)?;
    let folder_prefix = format!("{}/%", folder.display())
        .trim_start_matches('/')
        .to_owned();
    let sql = "SELECT '/' || relative_path, title, artist, bpm, musical_key
                 FROM tracks
                WHERE relative_path LIKE ?1
                  AND serato_missing = 0
                  AND bpm IS NOT NULL
                  AND musical_key IS NOT NULL
                ORDER BY relative_path";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([folder_prefix], |row| {
            Ok(CueportSeratoDbTrack {
                path: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                bpm: row.get(3)?,
                key: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().take(limit.unwrap_or(usize::MAX)).collect())
}

fn analyze_cueport_serato_track(track: &CueportSeratoDbTrack) -> CueportSeratoTrackResult {
    let path = Path::new(&track.path);
    let sha256 = file_sha256(path).ok();
    if !path.is_file() {
        return cueport_serato_error(
            track,
            sha256,
            CueportSeratoStatus::MissingFile,
            "missing file",
        );
    }

    let started = Instant::now();
    let decoded = match trailmix_codecs::decode_file(path) {
        Ok(decoded) => decoded,
        Err(error) => {
            return cueport_serato_error(
                track,
                sha256,
                CueportSeratoStatus::DecodeError,
                &error.to_string(),
            );
        }
    };
    let decode_ms = started.elapsed().as_secs_f64() * 1_000.0;

    let analysis_started = Instant::now();
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &decoded.samples,
            sample_rate: decoded.sample_rate,
        },
        AnalysisConfig::default(),
    );
    let analysis_ms = analysis_started.elapsed().as_secs_f64() * 1_000.0;

    let detected_bpm = analysis.beat.display_bpm;
    let Some(display_bpm) = detected_bpm else {
        return CueportSeratoTrackResult {
            path: track.path.clone(),
            sha256,
            title: track.title.clone(),
            artist: track.artist.clone(),
            serato_bpm: track.bpm,
            serato_key: track.key.clone(),
            trailmix_global_bpm: analysis.beat.global_bpm,
            trailmix_display_bpm: None,
            bpm_absolute_error: None,
            bpm_octave_aware_error: None,
            trailmix_key: analysis.key.key.map(|key| key.to_string()),
            exact_key_match: None,
            mirex_score: None,
            decode_milliseconds: Some(decode_ms),
            analysis_milliseconds: Some(analysis_ms),
            status: CueportSeratoStatus::NoBpm,
            error: None,
        };
    };

    let detected_key = analysis.key.key;
    let serato_key = track
        .key
        .as_deref()
        .and_then(|value| parse_camelot_or_key(value).ok());
    let exact_key_match = serato_key.zip(detected_key).map(|(s, d)| s == d);
    let mirex_score = serato_key
        .zip(detected_key)
        .map(|(s, d)| mirex_key_score(s, d));

    CueportSeratoTrackResult {
        path: track.path.clone(),
        sha256,
        title: track.title.clone(),
        artist: track.artist.clone(),
        serato_bpm: track.bpm,
        serato_key: track.key.clone(),
        trailmix_global_bpm: analysis.beat.global_bpm,
        trailmix_display_bpm: Some(display_bpm),
        bpm_absolute_error: track.bpm.map(|bpm| (display_bpm - bpm).abs()),
        bpm_octave_aware_error: track.bpm.map(|bpm| octave_aware_error(bpm, display_bpm)),
        trailmix_key: detected_key.map(|key| key.to_string()),
        exact_key_match,
        mirex_score,
        decode_milliseconds: Some(decode_ms),
        analysis_milliseconds: Some(analysis_ms),
        status: CueportSeratoStatus::Analyzed,
        error: None,
    }
}

fn cueport_serato_error(
    track: &CueportSeratoDbTrack,
    sha256: Option<String>,
    status: CueportSeratoStatus,
    error: &str,
) -> CueportSeratoTrackResult {
    CueportSeratoTrackResult {
        path: track.path.clone(),
        sha256,
        title: track.title.clone(),
        artist: track.artist.clone(),
        serato_bpm: track.bpm,
        serato_key: track.key.clone(),
        trailmix_global_bpm: None,
        trailmix_display_bpm: None,
        bpm_absolute_error: None,
        bpm_octave_aware_error: None,
        trailmix_key: None,
        exact_key_match: None,
        mirex_score: None,
        decode_milliseconds: None,
        analysis_milliseconds: None,
        status,
        error: Some(error.to_owned()),
    }
}

fn cueport_serato_summary(results: &[CueportSeratoTrackResult]) -> CueportSeratoSummary {
    let bpm_errors = results
        .iter()
        .filter_map(|track| track.bpm_absolute_error)
        .collect::<Vec<_>>();
    let octave_errors = results
        .iter()
        .filter_map(|track| track.bpm_octave_aware_error)
        .collect::<Vec<_>>();
    let exact_keys = results
        .iter()
        .filter_map(|track| track.exact_key_match)
        .collect::<Vec<_>>();
    let mirex_scores = results
        .iter()
        .filter_map(|track| track.mirex_score)
        .collect::<Vec<_>>();

    CueportSeratoSummary {
        bpm_mean_absolute_error: mean_f32(&bpm_errors),
        bpm_median_absolute_error: median_f32(bpm_errors.clone()),
        bpm_octave_aware_mean_absolute_error: mean_f32(&octave_errors),
        bpm_within_one: bpm_errors.iter().filter(|error| **error <= 1.0).count(),
        bpm_octave_within_one: octave_errors.iter().filter(|error| **error <= 1.0).count(),
        key_exact_accuracy: bool_accuracy(&exact_keys),
        key_mirex_mean: mean_f32(&mirex_scores),
    }
}

fn file_sha256(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut hex, "{byte:02x}")?;
    }
    Ok(hex)
}

fn trailmix_git_sha() -> Option<String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest_dir.parent()?.parent()?;
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn median_f32(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let midpoint = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        f32::midpoint(values[midpoint - 1], values[midpoint])
    } else {
        values[midpoint]
    })
}

fn run_synthetic() -> SyntheticBenchmark {
    let cases = [90.0, 120.0, 128.0].into_iter().map(run_case).collect();
    SyntheticBenchmark {
        version: 1,
        sample_rate: SAMPLE_RATE,
        duration_seconds: DURATION_SECONDS,
        warmup_runs: WARMUP_RUNS,
        timed_runs: TIMED_RUNS,
        cases,
    }
}

fn run_case(expected_bpm: f32) -> SyntheticCase {
    let samples = synthetic_track(expected_bpm);
    let config = AnalysisConfig::default();
    let audio = AudioBuffer {
        samples: &samples,
        sample_rate: SAMPLE_RATE,
    };

    for _ in 0..WARMUP_RUNS {
        let _ = trailmix::analyze(audio, config);
    }
    let mut timings = Vec::with_capacity(TIMED_RUNS);
    let mut analysis = None;
    for _ in 0..TIMED_RUNS {
        let started = Instant::now();
        let result = trailmix::analyze(audio, config);
        timings.push(started.elapsed().as_secs_f64() * 1_000.0);
        analysis = Some(result);
    }
    let analysis = analysis.expect("TIMED_RUNS is at least one");

    SyntheticCase {
        expected_bpm,
        detected_bpm: analysis.beat.global_bpm,
        absolute_error: analysis
            .beat
            .global_bpm
            .map(|detected| (detected - expected_bpm).abs()),
        confidence: analysis.beat.confidence,
        median_elapsed_milliseconds: median_f64(&timings).unwrap_or(0.0),
    }
}

fn synthetic_track(bpm: f32) -> Vec<f32> {
    let length = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let beat_interval = (60.0 / bpm * SAMPLE_RATE as f32).round() as usize;
    let chord = [220.0_f32, 277.18, 329.63];
    let mut samples = (0..length)
        .map(|index| {
            chord
                .iter()
                .map(|frequency| {
                    (2.0 * std::f32::consts::PI * frequency * index as f32 / SAMPLE_RATE as f32)
                        .sin()
                })
                .sum::<f32>()
                / chord.len() as f32
                * 0.2
        })
        .collect::<Vec<_>>();

    for position in (0..samples.len()).step_by(beat_interval.max(1)) {
        for offset in 0..128 {
            if let Some(sample) = samples.get_mut(position + offset) {
                *sample += (1.0 - offset as f32 / 128.0) * 0.8;
            }
        }
    }
    samples
}

fn run_manifest(
    path: &Path,
    limit: Option<usize>,
    parallel: bool,
    fast: bool,
    onnx_beats_path: Option<&str>,
) -> Result<CorpusBenchmark, Box<dyn Error>> {
    let manifest = trailmix_manifest::load(path)?;
    let base_directory = path.parent().unwrap_or_else(|| Path::new("."));

    // Load ONNX session once if requested
    let onnx_session: Option<std::sync::Arc<std::sync::Mutex<ort::session::Session>>> =
        match onnx_beats_path {
            Some(model_path) => {
                let session = ort::session::Session::builder()
                    .map_err(|e| format!("ort session builder: {e}"))?
                    .commit_from_file(model_path)
                    .map_err(|e| format!("loading ONNX model {model_path}: {e}"))?;
                Some(std::sync::Arc::new(std::sync::Mutex::new(session)))
            }
            None => None,
        };

    let track_slice: &[TrackAnnotation] = match limit {
        Some(n) => &manifest.tracks[..n.min(manifest.tracks.len())],
        None => &manifest.tracks,
    };

    let analyze_fn = |track: &TrackAnnotation| {
        let mut result = if fast {
            analyze_manifest_track_fast(track, base_directory)
        } else {
            analyze_manifest_track(track, base_directory)
        };

        // If ONNX beats requested, re-run beat detection with the model
        if let Some(ref session_arc) = onnx_session {
            if result.beat_f1.is_none() || result.expected_bpm.is_some() {
                if let Ok(decoded) = decode_track(track, base_directory) {
                    let config = beat_salad::onnx_beat::OnnxBeatConfig::beat_this();
                    let mut session = session_arc.lock().unwrap();
                    if let Ok(beats) = beat_salad::onnx_beat::track_beats(
                        &mut session,
                        &decoded.samples,
                        decoded.sample_rate,
                        &config,
                    ) {
                        let beat_scores = beat_position_f1(&track.expected_beats, &beats, 0.070);
                        result.beat_f1 = beat_scores.map(|s| s.f1);
                        result.beat_precision = beat_scores.map(|s| s.precision);
                        result.beat_recall = beat_scores.map(|s| s.recall);
                    }
                }
            }
        }

        result
    };

    let tracks: Vec<TrackResult> = if parallel && onnx_session.is_none() {
        track_slice.par_iter().map(analyze_fn).collect()
    } else {
        // ONNX session is behind a Mutex; sequential is simpler and avoids contention
        track_slice.iter().map(analyze_fn).collect()
    };

    let summary = summarize(&tracks);

    Ok(CorpusBenchmark {
        version: 2,
        manifest_version: manifest.version,
        track_count: tracks.len(),
        warmup_runs: if fast { 0 } else { WARMUP_RUNS },
        timed_runs: if fast { 1 } else { TIMED_RUNS },
        summary,
        tracks,
    })
}

/// Timed, repeated measurements of the three analyzers called by
/// `trailmix::analyze`. Timing each analyzer separately turns one opaque total into a
/// breakdown that can be attributed to a component. The analyzers are pure functions
/// of their input samples, so repeating a run changes only the timings.
struct AnalysisTimings {
    beat: f64,
    key: f64,
    waveform: f64,
}

fn measure_analyzers(samples: &[f32], sample_rate: u32, config: AnalysisConfig) -> AnalysisTimings {
    for _ in 0..WARMUP_RUNS {
        let _ = beat_salad::analyze(samples, sample_rate, config.beat);
        let _ = key_lime::analyze(samples, sample_rate, config.key);
        let _ = sampler_platter::generate_overview(samples, sample_rate, config.waveform_columns);
    }

    let mut beat_timings = Vec::with_capacity(TIMED_RUNS);
    let mut key_timings = Vec::with_capacity(TIMED_RUNS);
    let mut waveform_timings = Vec::with_capacity(TIMED_RUNS);

    for _ in 0..TIMED_RUNS {
        let beat_started = Instant::now();
        let _ = beat_salad::analyze(samples, sample_rate, config.beat);
        beat_timings.push(beat_started.elapsed().as_secs_f64() * 1_000.0);

        let key_started = Instant::now();
        let _ = key_lime::analyze(samples, sample_rate, config.key);
        key_timings.push(key_started.elapsed().as_secs_f64() * 1_000.0);

        let waveform_started = Instant::now();
        let _ = sampler_platter::generate_overview(samples, sample_rate, config.waveform_columns);
        waveform_timings.push(waveform_started.elapsed().as_secs_f64() * 1_000.0);
    }

    AnalysisTimings {
        beat: median_f64(&beat_timings).unwrap_or(0.0),
        key: median_f64(&key_timings).unwrap_or(0.0),
        waveform: median_f64(&waveform_timings).unwrap_or(0.0),
    }
}

fn analyze_manifest_track(track: &TrackAnnotation, base_directory: &Path) -> TrackResult {
    let path = if track.path.is_absolute() {
        track.path.clone()
    } else {
        base_directory.join(&track.path)
    };
    let decode_started = Instant::now();
    let decoded = match trailmix_codecs::decode_file(path) {
        Ok(decoded) => decoded,
        Err(error) => return failed_track(track, error.to_string()),
    };
    let decode_milliseconds = decode_started.elapsed().as_secs_f64() * 1_000.0;
    let config = AnalysisConfig::default();
    let timings = measure_analyzers(&decoded.samples, decoded.sample_rate, config);
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &decoded.samples,
            sample_rate: decoded.sample_rate,
        },
        config,
    );
    let analysis_milliseconds = timings.beat + timings.key + timings.waveform;

    let expected_key = match track.expected_key.as_deref().map(parse_key).transpose() {
        Ok(key) => key,
        Err(error) => return failed_track(track, error),
    };
    let detected_key = analysis.key.key;
    let bpm_absolute_error = paired_bpm(track.expected_bpm, analysis.beat.global_bpm)
        .map(|(expected, detected)| (detected - expected).abs());
    let bpm_octave_aware_absolute_error = paired_bpm(track.expected_bpm, analysis.beat.global_bpm)
        .map(|(expected, detected)| octave_aware_error(expected, detected));
    let key_segment_exact_accuracy =
        match key_segment_accuracy(&track.expected_key_segments, &analysis) {
            Ok(accuracy) => accuracy,
            Err(error) => return failed_track(track, error),
        };
    let beat_scores = beat_position_f1(&track.expected_beats, &analysis.beat.beats, 0.070);
    let serato_scores = score_serato_agreement(
        track.serato.as_ref(),
        analysis.beat.global_bpm,
        analysis.key.key,
    );
    let apple_music_understanding_scores = score_apple_music_understanding_agreement(
        track.apple_music_understanding.as_ref(),
        analysis.beat.global_bpm,
        analysis.key.key,
    );

    TrackResult {
        id: track.id.clone(),
        split: track.split.clone(),
        duration_seconds: Some(decoded.duration_seconds()),
        decode_milliseconds: Some(decode_milliseconds),
        analysis_milliseconds: Some(analysis_milliseconds),
        beat_analysis_milliseconds: Some(timings.beat),
        key_analysis_milliseconds: Some(timings.key),
        waveform_analysis_milliseconds: Some(timings.waveform),
        expected_bpm: track.expected_bpm,
        detected_bpm: analysis.beat.global_bpm,
        bpm_absolute_error,
        bpm_octave_aware_absolute_error,
        expected_key: expected_key.map(|key| key.to_string()),
        detected_key: detected_key.map(|key| key.to_string()),
        exact_key_match: expected_key.map(|expected| Some(expected) == detected_key),
        mirex_score: expected_key
            .zip(detected_key)
            .map(|(expected, detected)| mirex_key_score(expected, detected)),
        tempo_segment_mean_absolute_error: tempo_segment_error(
            &track.expected_tempo_segments,
            &analysis,
        ),
        key_segment_exact_accuracy,
        beat_f1: beat_scores.map(|scores| scores.f1),
        beat_precision: beat_scores.map(|scores| scores.precision),
        beat_recall: beat_scores.map(|scores| scores.recall),
        multi_tempo: analysis.beat.multi_tempo,
        alternate_bpm: analysis.beat.alternate_bpm,
        alternate_bpm_coverage: analysis.beat.alternate_coverage,
        multi_key: analysis.key.multi_key,
        alternate_key: analysis.key.alternate_key.map(|key| key.to_string()),
        alternate_key_coverage: analysis.key.alternate_coverage,
        serato_bpm: track.serato.as_ref().and_then(|serato| serato.bpm),
        serato_key: track.serato.as_ref().and_then(|serato| serato.key.clone()),
        serato_bpm_absolute_agreement: serato_scores.bpm_absolute,
        serato_bpm_octave_aware_absolute_agreement: serato_scores.bpm_octave_aware,
        serato_exact_key_agreement: serato_scores.exact_key,
        apple_music_understanding_bpm: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.bpm),
        apple_music_understanding_key: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.key.clone()),
        apple_music_understanding_bpm_absolute_agreement: apple_music_understanding_scores
            .bpm_absolute,
        apple_music_understanding_bpm_octave_aware_absolute_agreement:
            apple_music_understanding_scores.bpm_octave_aware,
        apple_music_understanding_exact_key_agreement: apple_music_understanding_scores.exact_key,
        chroma: Some(analysis.key.chroma),
        error: None,
    }
}

/// Single-pass analysis without warmup or repeated timing. Used with --fast.
fn analyze_manifest_track_fast(track: &TrackAnnotation, base_directory: &Path) -> TrackResult {
    let decoded = match decode_track(track, base_directory) {
        Ok(d) => d,
        Err(error) => return failed_track(track, error),
    };
    let config = AnalysisConfig::default();
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &decoded.samples,
            sample_rate: decoded.sample_rate,
        },
        config,
    );

    let expected_key = match track.expected_key.as_deref().map(parse_key).transpose() {
        Ok(key) => key,
        Err(error) => return failed_track(track, error),
    };
    let detected_key = analysis.key.key;
    let bpm_absolute_error = paired_bpm(track.expected_bpm, analysis.beat.global_bpm)
        .map(|(expected, detected)| (detected - expected).abs());
    let bpm_octave_aware_absolute_error = paired_bpm(track.expected_bpm, analysis.beat.global_bpm)
        .map(|(expected, detected)| octave_aware_error(expected, detected));
    let key_segment_exact_accuracy =
        match key_segment_accuracy(&track.expected_key_segments, &analysis) {
            Ok(accuracy) => accuracy,
            Err(error) => return failed_track(track, error),
        };
    let beat_scores = beat_position_f1(&track.expected_beats, &analysis.beat.beats, 0.070);
    let serato_scores = score_serato_agreement(
        track.serato.as_ref(),
        analysis.beat.global_bpm,
        analysis.key.key,
    );
    let apple_music_understanding_scores = score_apple_music_understanding_agreement(
        track.apple_music_understanding.as_ref(),
        analysis.beat.global_bpm,
        analysis.key.key,
    );

    TrackResult {
        id: track.id.clone(),
        split: track.split.clone(),
        duration_seconds: Some(decoded.duration_seconds()),
        decode_milliseconds: None,
        analysis_milliseconds: None,
        beat_analysis_milliseconds: None,
        key_analysis_milliseconds: None,
        waveform_analysis_milliseconds: None,
        expected_bpm: track.expected_bpm,
        detected_bpm: analysis.beat.global_bpm,
        bpm_absolute_error,
        bpm_octave_aware_absolute_error,
        expected_key: expected_key.map(|key| key.to_string()),
        detected_key: detected_key.map(|key| key.to_string()),
        exact_key_match: expected_key.map(|expected| Some(expected) == detected_key),
        mirex_score: expected_key
            .zip(detected_key)
            .map(|(expected, detected)| mirex_key_score(expected, detected)),
        tempo_segment_mean_absolute_error: tempo_segment_error(
            &track.expected_tempo_segments,
            &analysis,
        ),
        key_segment_exact_accuracy,
        beat_f1: beat_scores.map(|scores| scores.f1),
        beat_precision: beat_scores.map(|scores| scores.precision),
        beat_recall: beat_scores.map(|scores| scores.recall),
        multi_tempo: analysis.beat.multi_tempo,
        alternate_bpm: analysis.beat.alternate_bpm,
        alternate_bpm_coverage: analysis.beat.alternate_coverage,
        multi_key: analysis.key.multi_key,
        alternate_key: analysis.key.alternate_key.map(|key| key.to_string()),
        alternate_key_coverage: analysis.key.alternate_coverage,
        serato_bpm: track.serato.as_ref().and_then(|serato| serato.bpm),
        serato_key: track.serato.as_ref().and_then(|serato| serato.key.clone()),
        serato_bpm_absolute_agreement: serato_scores.bpm_absolute,
        serato_bpm_octave_aware_absolute_agreement: serato_scores.bpm_octave_aware,
        serato_exact_key_agreement: serato_scores.exact_key,
        apple_music_understanding_bpm: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.bpm),
        apple_music_understanding_key: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.key.clone()),
        apple_music_understanding_bpm_absolute_agreement: apple_music_understanding_scores
            .bpm_absolute,
        apple_music_understanding_bpm_octave_aware_absolute_agreement:
            apple_music_understanding_scores.bpm_octave_aware,
        apple_music_understanding_exact_key_agreement: apple_music_understanding_scores.exact_key,
        chroma: Some(analysis.key.chroma),
        error: None,
    }
}

fn decode_track(
    track: &TrackAnnotation,
    base_directory: &Path,
) -> Result<trailmix_codecs::DecodedAudio, String> {
    let path = if track.path.is_absolute() {
        track.path.clone()
    } else {
        base_directory.join(&track.path)
    };
    trailmix_codecs::decode_file(path).map_err(|e| e.to_string())
}

fn failed_track(track: &TrackAnnotation, error: String) -> TrackResult {
    TrackResult {
        id: track.id.clone(),
        split: track.split.clone(),
        duration_seconds: None,
        decode_milliseconds: None,
        analysis_milliseconds: None,
        beat_analysis_milliseconds: None,
        key_analysis_milliseconds: None,
        waveform_analysis_milliseconds: None,
        expected_bpm: track.expected_bpm,
        detected_bpm: None,
        bpm_absolute_error: None,
        bpm_octave_aware_absolute_error: None,
        expected_key: track.expected_key.clone(),
        detected_key: None,
        exact_key_match: None,
        mirex_score: None,
        tempo_segment_mean_absolute_error: None,
        key_segment_exact_accuracy: None,
        beat_f1: None,
        beat_precision: None,
        beat_recall: None,
        multi_tempo: false,
        alternate_bpm: None,
        alternate_bpm_coverage: 0.0,
        multi_key: false,
        alternate_key: None,
        alternate_key_coverage: 0.0,
        serato_bpm: track.serato.as_ref().and_then(|serato| serato.bpm),
        serato_key: track.serato.as_ref().and_then(|serato| serato.key.clone()),
        serato_bpm_absolute_agreement: None,
        serato_bpm_octave_aware_absolute_agreement: None,
        serato_exact_key_agreement: None,
        apple_music_understanding_bpm: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.bpm),
        apple_music_understanding_key: track
            .apple_music_understanding
            .as_ref()
            .and_then(|observation| observation.key.clone()),
        apple_music_understanding_bpm_absolute_agreement: None,
        apple_music_understanding_bpm_octave_aware_absolute_agreement: None,
        apple_music_understanding_exact_key_agreement: None,
        chroma: None,
        error: Some(error),
    }
}

fn paired_bpm(expected: Option<f32>, detected: Option<f32>) -> Option<(f32, f32)> {
    expected.zip(detected)
}

fn octave_aware_error(expected: f32, detected: f32) -> f32 {
    [
        (detected - expected).abs(),
        (detected * 2.0 - expected).abs(),
        (detected / 2.0 - expected).abs(),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

fn tempo_segment_error(expected: &[TempoSegmentAnnotation], analysis: &Analysis) -> Option<f32> {
    if expected.is_empty() {
        return None;
    }
    let errors = expected
        .iter()
        .filter_map(|expected_segment| {
            let midpoint =
                f64::midpoint(expected_segment.start_seconds, expected_segment.end_seconds);
            analysis
                .beat
                .tempo_segments
                .iter()
                .find(|detected| {
                    midpoint >= detected.start_seconds && midpoint < detected.end_seconds
                })
                .map(|detected| (detected.bpm - expected_segment.bpm).abs())
        })
        .collect::<Vec<_>>();
    mean_f32(&errors)
}

fn key_segment_accuracy(
    expected: &[KeySegmentAnnotation],
    analysis: &Analysis,
) -> Result<Option<f32>, String> {
    if expected.is_empty() {
        return Ok(None);
    }

    let mut annotated_seconds = 0.0;
    let mut matching_seconds = 0.0;
    for expected_segment in expected {
        let expected_key = parse_key(&expected_segment.key)?;
        annotated_seconds += expected_segment.end_seconds - expected_segment.start_seconds;
        for detected in &analysis.key.segments {
            let overlap_start = expected_segment.start_seconds.max(detected.start_seconds);
            let overlap_end = expected_segment.end_seconds.min(detected.end_seconds);
            if overlap_end > overlap_start && detected.key == expected_key {
                matching_seconds += overlap_end - overlap_start;
            }
        }
    }

    Ok(Some(
        (matching_seconds / annotated_seconds.max(f64::EPSILON)) as f32,
    ))
}

struct SeratoAgreement {
    bpm_absolute: Option<f32>,
    bpm_octave_aware: Option<f32>,
    exact_key: Option<bool>,
}

fn score_serato_agreement(
    serato: Option<&SeratoObservation>,
    detected_bpm: Option<f32>,
    detected_key: Option<MusicalKey>,
) -> SeratoAgreement {
    let Some(serato) = serato else {
        return SeratoAgreement {
            bpm_absolute: None,
            bpm_octave_aware: None,
            exact_key: None,
        };
    };

    let bpm_absolute = paired_bpm(serato.bpm, detected_bpm)
        .map(|(serato_bpm, trail_bpm)| (trail_bpm - serato_bpm).abs());
    let bpm_octave_aware = paired_bpm(serato.bpm, detected_bpm)
        .map(|(serato_bpm, trail_bpm)| octave_aware_error(serato_bpm, trail_bpm));

    let exact_key = serato
        .key
        .as_deref()
        .and_then(|key_str| parse_camelot_or_key(key_str).ok())
        .map(|serato_key| Some(serato_key) == detected_key);

    SeratoAgreement {
        bpm_absolute,
        bpm_octave_aware,
        exact_key,
    }
}

fn score_apple_music_understanding_agreement(
    observation: Option<&AppleMusicUnderstandingObservation>,
    detected_bpm: Option<f32>,
    detected_key: Option<MusicalKey>,
) -> SeratoAgreement {
    let Some(observation) = observation else {
        return SeratoAgreement {
            bpm_absolute: None,
            bpm_octave_aware: None,
            exact_key: None,
        };
    };

    let bpm_absolute = paired_bpm(observation.bpm, detected_bpm)
        .map(|(reference_bpm, trail_bpm)| (trail_bpm - reference_bpm).abs());
    let bpm_octave_aware = paired_bpm(observation.bpm, detected_bpm)
        .map(|(reference_bpm, trail_bpm)| octave_aware_error(reference_bpm, trail_bpm));
    let exact_key = observation
        .key
        .as_deref()
        .and_then(|key_str| parse_key(key_str).ok())
        .map(|reference_key| Some(reference_key) == detected_key);

    SeratoAgreement {
        bpm_absolute,
        bpm_octave_aware,
        exact_key,
    }
}

/// Parse a key string that may be Camelot (e.g. "8A"), standard (e.g. "A minor"), or
/// Open Key notation.
fn parse_camelot_or_key(value: &str) -> Result<MusicalKey, String> {
    let trimmed = value.trim();
    if let Some(key) = camelot_to_key(trimmed) {
        return Ok(key);
    }
    parse_key(trimmed)
}

#[allow(clippy::match_same_arms)]
fn camelot_to_key(value: &str) -> Option<MusicalKey> {
    let value = value.trim().to_uppercase();
    let (number, mode_char) = if value.ends_with('A') || value.ends_with('B') {
        let mode_char = value.as_bytes().last()?;
        let number: u8 = value[..value.len() - 1].parse().ok()?;
        (number, *mode_char)
    } else {
        return None;
    };
    if !(1..=12).contains(&number) {
        return None;
    }
    let mode = if mode_char == b'A' {
        Mode::Minor
    } else {
        Mode::Major
    };
    let tonic = match (number, mode) {
        (1, Mode::Minor) => PitchClass::GSharp,
        (2, Mode::Minor) => PitchClass::DSharp,
        (3, Mode::Minor) => PitchClass::ASharp,
        (4, Mode::Minor) => PitchClass::F,
        (5, Mode::Minor) => PitchClass::C,
        (6, Mode::Minor) => PitchClass::G,
        (7, Mode::Minor) => PitchClass::D,
        (8, Mode::Minor) => PitchClass::A,
        (9, Mode::Minor) => PitchClass::E,
        (10, Mode::Minor) => PitchClass::B,
        (11, Mode::Minor) => PitchClass::FSharp,
        (12, Mode::Minor) => PitchClass::CSharp,
        (1, Mode::Major) => PitchClass::B,
        (2, Mode::Major) => PitchClass::FSharp,
        (3, Mode::Major) => PitchClass::CSharp,
        (4, Mode::Major) => PitchClass::GSharp,
        (5, Mode::Major) => PitchClass::DSharp,
        (6, Mode::Major) => PitchClass::ASharp,
        (7, Mode::Major) => PitchClass::F,
        (8, Mode::Major) => PitchClass::C,
        (9, Mode::Major) => PitchClass::G,
        (10, Mode::Major) => PitchClass::D,
        (11, Mode::Major) => PitchClass::A,
        (12, Mode::Major) => PitchClass::E,
        _ => return None,
    };
    Some(MusicalKey { tonic, mode })
}

/// MIREX-style weighted key evaluation score.
/// exact = 1.0, fifth = 0.5, relative = 0.3, parallel = 0.2, other = 0.0
fn mirex_key_score(expected: MusicalKey, detected: MusicalKey) -> f32 {
    if expected == detected {
        return 1.0;
    }
    let exp_pc = expected.tonic as i8;
    let det_pc = detected.tonic as i8;
    let interval = ((det_pc - exp_pc).rem_euclid(12)) as u8;

    if expected.mode == detected.mode && interval == 7 {
        return 0.5;
    }

    let is_relative = match expected.mode {
        Mode::Major => detected.mode == Mode::Minor && interval == 9,
        Mode::Minor => detected.mode == Mode::Major && interval == 3,
    };
    if is_relative {
        return 0.3;
    }

    if expected.tonic == detected.tonic && expected.mode != detected.mode {
        return 0.2;
    }

    0.0
}

#[derive(Clone, Copy)]
struct BeatF1Scores {
    precision: f32,
    recall: f32,
    f1: f32,
}

/// Beat-position F1 with a tolerance window (seconds). Each reference beat is matched
/// to at most one detected beat (nearest within tolerance), and vice versa.
fn beat_position_f1(
    expected: &[BeatAnnotation],
    detected: &[BeatPosition],
    tolerance_seconds: f64,
) -> Option<BeatF1Scores> {
    if expected.is_empty() {
        return None;
    }
    if detected.is_empty() {
        return Some(BeatF1Scores {
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
        });
    }

    let mut detected_matched = vec![false; detected.len()];
    let mut true_positives = 0u32;

    for reference in expected {
        let mut best_index = None;
        let mut best_distance = f64::INFINITY;
        for (index, beat) in detected.iter().enumerate() {
            if detected_matched[index] {
                continue;
            }
            let distance = (beat.time_seconds - reference.time_seconds).abs();
            if distance <= tolerance_seconds && distance < best_distance {
                best_distance = distance;
                best_index = Some(index);
            }
        }
        if let Some(index) = best_index {
            detected_matched[index] = true;
            true_positives += 1;
        }
    }

    let precision = true_positives as f32 / detected.len() as f32;
    let recall = true_positives as f32 / expected.len() as f32;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    Some(BeatF1Scores {
        precision,
        recall,
        f1,
    })
}

#[allow(clippy::too_many_lines)]
fn summarize(tracks: &[TrackResult]) -> CorpusSummary {
    let bpm_errors = tracks
        .iter()
        .filter_map(|track| track.bpm_absolute_error)
        .collect::<Vec<_>>();
    let octave_errors = tracks
        .iter()
        .filter_map(|track| track.bpm_octave_aware_absolute_error)
        .collect::<Vec<_>>();
    let key_matches = tracks
        .iter()
        .filter_map(|track| track.exact_key_match)
        .collect::<Vec<_>>();
    let mirex_scores = tracks
        .iter()
        .filter_map(|track| track.mirex_score)
        .collect::<Vec<_>>();
    let segment_errors = tracks
        .iter()
        .filter_map(|track| track.tempo_segment_mean_absolute_error)
        .collect::<Vec<_>>();
    let key_segment_accuracies = tracks
        .iter()
        .filter_map(|track| track.key_segment_exact_accuracy)
        .collect::<Vec<_>>();
    let decode_times = tracks
        .iter()
        .filter_map(|track| track.decode_milliseconds)
        .collect::<Vec<_>>();
    let analysis_times = tracks
        .iter()
        .filter_map(|track| track.analysis_milliseconds)
        .collect::<Vec<_>>();
    let beat_times = tracks
        .iter()
        .filter_map(|track| track.beat_analysis_milliseconds)
        .collect::<Vec<_>>();
    let key_times = tracks
        .iter()
        .filter_map(|track| track.key_analysis_milliseconds)
        .collect::<Vec<_>>();
    let waveform_times = tracks
        .iter()
        .filter_map(|track| track.waveform_analysis_milliseconds)
        .collect::<Vec<_>>();

    let serato_bpm_errors = tracks
        .iter()
        .filter_map(|track| track.serato_bpm_absolute_agreement)
        .collect::<Vec<_>>();
    let serato_bpm_octave_errors = tracks
        .iter()
        .filter_map(|track| track.serato_bpm_octave_aware_absolute_agreement)
        .collect::<Vec<_>>();
    let serato_key_matches = tracks
        .iter()
        .filter_map(|track| track.serato_exact_key_agreement)
        .collect::<Vec<_>>();
    let apple_music_understanding_bpm_errors = tracks
        .iter()
        .filter_map(|track| track.apple_music_understanding_bpm_absolute_agreement)
        .collect::<Vec<_>>();
    let apple_music_understanding_bpm_octave_errors = tracks
        .iter()
        .filter_map(|track| track.apple_music_understanding_bpm_octave_aware_absolute_agreement)
        .collect::<Vec<_>>();
    let apple_music_understanding_key_matches = tracks
        .iter()
        .filter_map(|track| track.apple_music_understanding_exact_key_agreement)
        .collect::<Vec<_>>();
    let beat_f1_values = tracks
        .iter()
        .filter_map(|track| track.beat_f1)
        .collect::<Vec<_>>();
    let beat_precision_values = tracks
        .iter()
        .filter_map(|track| track.beat_precision)
        .collect::<Vec<_>>();
    let beat_recall_values = tracks
        .iter()
        .filter_map(|track| track.beat_recall)
        .collect::<Vec<_>>();

    CorpusSummary {
        analyzed_tracks: tracks.iter().filter(|track| track.error.is_none()).count(),
        failed_tracks: tracks.iter().filter(|track| track.error.is_some()).count(),
        bpm_mean_absolute_error: mean_f32(&bpm_errors),
        bpm_octave_aware_mean_absolute_error: mean_f32(&octave_errors),
        exact_key_accuracy: mean_f32(
            &key_matches
                .iter()
                .map(|matches| f32::from(u8::from(*matches)))
                .collect::<Vec<_>>(),
        ),
        mirex_weighted_score: mean_f32(&mirex_scores),
        tempo_segment_mean_absolute_error: mean_f32(&segment_errors),
        key_segment_exact_accuracy: mean_f32(&key_segment_accuracies),
        beat_f1: mean_f32(&beat_f1_values),
        beat_precision: mean_f32(&beat_precision_values),
        beat_recall: mean_f32(&beat_recall_values),
        multi_tempo_tracks: tracks.iter().filter(|track| track.multi_tempo).count(),
        multi_key_tracks: tracks.iter().filter(|track| track.multi_key).count(),
        serato_bpm_mean_absolute_agreement: mean_f32(&serato_bpm_errors),
        serato_bpm_octave_aware_mean_absolute_agreement: mean_f32(&serato_bpm_octave_errors),
        serato_exact_key_agreement: mean_f32(
            &serato_key_matches
                .iter()
                .map(|matches| f32::from(u8::from(*matches)))
                .collect::<Vec<_>>(),
        ),
        apple_music_understanding_bpm_mean_absolute_agreement: mean_f32(
            &apple_music_understanding_bpm_errors,
        ),
        apple_music_understanding_bpm_octave_aware_mean_absolute_agreement: mean_f32(
            &apple_music_understanding_bpm_octave_errors,
        ),
        apple_music_understanding_exact_key_agreement: mean_f32(
            &apple_music_understanding_key_matches
                .iter()
                .map(|matches| f32::from(u8::from(*matches)))
                .collect::<Vec<_>>(),
        ),
        mean_decode_milliseconds: mean_f64(&decode_times),
        mean_analysis_milliseconds: mean_f64(&analysis_times),
        mean_beat_analysis_milliseconds: mean_f64(&beat_times),
        mean_key_analysis_milliseconds: mean_f64(&key_times),
        mean_waveform_analysis_milliseconds: mean_f64(&waveform_times),
    }
}

fn mean_f32(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

fn bool_accuracy(values: &[bool]) -> Option<f32> {
    (!values.is_empty())
        .then(|| values.iter().filter(|value| **value).count() as f32 / values.len() as f32)
}

fn mean_f64(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Median of timed-run milliseconds. This limits the effect of an occasional slow
/// run caused by OS scheduling.
fn median_f64(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let midpoint = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        f64::midpoint(sorted[midpoint - 1], sorted[midpoint])
    } else {
        sorted[midpoint]
    })
}

fn parse_key(value: &str) -> Result<MusicalKey, String> {
    let normalized = value.trim().replace('♯', "#").replace('♭', "b");
    let (tonic, mode) = if normalized.split_whitespace().count() == 1 {
        split_compact_key(&normalized)?
    } else {
        let mut parts = normalized.split_whitespace();
        let tonic = parts
            .next()
            .ok_or_else(|| format!("invalid expected key: {value}"))?;
        let mode = parts
            .next()
            .ok_or_else(|| format!("expected key must include major or minor: {value}"))?;
        if parts.next().is_some() {
            return Err(format!("invalid expected key: {value}"));
        }
        (tonic.to_owned(), mode.to_owned())
    };

    let tonic = match tonic.to_ascii_uppercase().as_str() {
        "C" => PitchClass::C,
        "C#" | "DB" => PitchClass::CSharp,
        "D" => PitchClass::D,
        "D#" | "EB" => PitchClass::DSharp,
        "E" | "FB" => PitchClass::E,
        "F" | "E#" => PitchClass::F,
        "F#" | "GB" => PitchClass::FSharp,
        "G" => PitchClass::G,
        "G#" | "AB" => PitchClass::GSharp,
        "A" => PitchClass::A,
        "A#" | "BB" => PitchClass::ASharp,
        "B" | "CB" => PitchClass::B,
        _ => return Err(format!("invalid expected key tonic: {tonic}")),
    };
    let mode = match mode.to_ascii_lowercase().as_str() {
        "major" | "maj" => Mode::Major,
        "minor" | "min" | "m" => Mode::Minor,
        _ => return Err(format!("invalid expected key mode: {mode}")),
    };
    Ok(MusicalKey { tonic, mode })
}

fn split_compact_key(value: &str) -> Result<(String, String), String> {
    let compact = value.trim();
    if compact.is_empty() {
        return Err("invalid expected key".to_owned());
    }
    if let Some(tonic) = compact.strip_suffix('m') {
        return Ok((tonic.to_owned(), "minor".to_owned()));
    }
    if let Some(tonic) = compact.strip_suffix("min") {
        return Ok((tonic.to_owned(), "minor".to_owned()));
    }
    if let Some(tonic) = compact.strip_suffix("maj") {
        return Ok((tonic.to_owned(), "major".to_owned()));
    }
    Ok((compact.to_owned(), "major".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use hound::{SampleFormat, WavSpec, WavWriter};

    use super::*;

    #[test]
    fn parses_flat_minor_key() {
        assert_eq!(
            parse_key("Eb minor").expect("key"),
            MusicalKey {
                tonic: PitchClass::DSharp,
                mode: Mode::Minor,
            }
        );
    }

    #[test]
    fn parses_compact_serato_minor_key() {
        assert_eq!(
            parse_key("Abm").expect("key"),
            MusicalKey {
                tonic: PitchClass::GSharp,
                mode: Mode::Minor,
            }
        );
    }

    #[test]
    fn parses_compact_serato_major_key() {
        assert_eq!(
            parse_key("F#").expect("key"),
            MusicalKey {
                tonic: PitchClass::FSharp,
                mode: Mode::Major,
            }
        );
    }

    #[test]
    fn parses_camelot_8a_as_a_minor() {
        assert_eq!(
            camelot_to_key("8A"),
            Some(MusicalKey {
                tonic: PitchClass::A,
                mode: Mode::Minor,
            })
        );
    }

    #[test]
    fn parses_camelot_8b_as_c_major() {
        assert_eq!(
            camelot_to_key("8B"),
            Some(MusicalKey {
                tonic: PitchClass::C,
                mode: Mode::Major,
            })
        );
    }

    #[test]
    fn parses_camelot_1a_as_g_sharp_minor() {
        assert_eq!(
            camelot_to_key("1A"),
            Some(MusicalKey {
                tonic: PitchClass::GSharp,
                mode: Mode::Minor,
            })
        );
    }

    #[test]
    fn rejects_invalid_camelot() {
        assert_eq!(camelot_to_key("13A"), None);
        assert_eq!(camelot_to_key("0B"), None);
        assert_eq!(camelot_to_key("foo"), None);
    }

    #[test]
    fn beat_f1_perfect_match() {
        let expected = vec![
            BeatAnnotation {
                time_seconds: 0.5,
                position_in_bar: None,
            },
            BeatAnnotation {
                time_seconds: 1.0,
                position_in_bar: None,
            },
            BeatAnnotation {
                time_seconds: 1.5,
                position_in_bar: None,
            },
        ];
        let detected = vec![
            BeatPosition {
                time_seconds: 0.5,
                confidence: 0.8,
                position_in_bar: 1,
            },
            BeatPosition {
                time_seconds: 1.0,
                confidence: 0.8,
                position_in_bar: 2,
            },
            BeatPosition {
                time_seconds: 1.5,
                confidence: 0.8,
                position_in_bar: 3,
            },
        ];
        let scores = beat_position_f1(&expected, &detected, 0.070).unwrap();
        assert!((scores.f1 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn beat_f1_with_offset() {
        let expected = vec![
            BeatAnnotation {
                time_seconds: 0.5,
                position_in_bar: None,
            },
            BeatAnnotation {
                time_seconds: 1.0,
                position_in_bar: None,
            },
        ];
        let detected = vec![
            BeatPosition {
                time_seconds: 0.55,
                confidence: 0.8,
                position_in_bar: 1,
            },
            BeatPosition {
                time_seconds: 1.05,
                confidence: 0.8,
                position_in_bar: 2,
            },
        ];
        let scores = beat_position_f1(&expected, &detected, 0.070).unwrap();
        assert!((scores.f1 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn beat_f1_outside_tolerance() {
        let expected = vec![BeatAnnotation {
            time_seconds: 0.5,
            position_in_bar: None,
        }];
        let detected = vec![BeatPosition {
            time_seconds: 0.6,
            confidence: 0.8,
            position_in_bar: 1,
        }];
        let scores = beat_position_f1(&expected, &detected, 0.070).unwrap();
        assert!((scores.f1 - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn treats_half_time_as_octave_equivalent() {
        assert!((octave_aware_error(128.0, 64.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn mirex_scores_key_relationships() {
        let c_major = MusicalKey {
            tonic: PitchClass::C,
            mode: Mode::Major,
        };
        let a_minor = MusicalKey {
            tonic: PitchClass::A,
            mode: Mode::Minor,
        };
        let g_major = MusicalKey {
            tonic: PitchClass::G,
            mode: Mode::Major,
        };
        let c_minor = MusicalKey {
            tonic: PitchClass::C,
            mode: Mode::Minor,
        };
        let d_major = MusicalKey {
            tonic: PitchClass::D,
            mode: Mode::Major,
        };

        assert!((mirex_key_score(c_major, c_major) - 1.0).abs() < f32::EPSILON);
        assert!((mirex_key_score(c_major, g_major) - 0.5).abs() < f32::EPSILON);
        assert!((mirex_key_score(c_major, a_minor) - 0.3).abs() < f32::EPSILON);
        assert!((mirex_key_score(c_major, c_minor) - 0.2).abs() < f32::EPSILON);
        assert!((mirex_key_score(c_major, d_major) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scores_exact_local_key_overlap() {
        let sample_rate = 8_000;
        let samples = (0..sample_rate * 4)
            .map(|index| {
                [220.0_f32, 277.18, 329.63]
                    .iter()
                    .map(|frequency| {
                        (2.0 * std::f32::consts::PI * frequency * index as f32 / sample_rate as f32)
                            .sin()
                    })
                    .sum::<f32>()
                    / 3.0
            })
            .collect::<Vec<_>>();
        let analysis = trailmix::analyze(
            AudioBuffer {
                samples: &samples,
                sample_rate,
            },
            AnalysisConfig::default(),
        );
        let accuracy = key_segment_accuracy(
            &[KeySegmentAnnotation {
                start_seconds: 0.0,
                end_seconds: 4.0,
                key: "A major".to_owned(),
                confidence: None,
            }],
            &analysis,
        )
        .expect("valid key")
        .expect("accuracy");

        assert!((accuracy - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn evaluates_a_manifest_without_exposing_source_path() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = env::temp_dir().join(format!("trailmix-benchmark-{nonce}"));
        fs::create_dir(&directory).expect("create fixture directory");
        let audio_path = directory.join("private-audio.wav");
        let manifest_path = directory.join("manifest.json");

        let mut writer = WavWriter::create(
            &audio_path,
            WavSpec {
                channels: 1,
                sample_rate: 8_000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .expect("create WAV");
        for index in 0..32_000 {
            let within_beat = index % 4_000;
            let sample = if within_beat < 64 {
                i16::MAX - i16::try_from(within_beat * 400).expect("sample")
            } else {
                0
            };
            writer.write_sample(sample).expect("write sample");
        }
        writer.finalize().expect("finalize WAV");
        fs::write(
            &manifest_path,
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "tracks": [{
                    "id": "fixture",
                    "path": "private-audio.wav",
                    "expected_bpm": 120.0
                }]
            }))
            .expect("serialize manifest"),
        )
        .expect("write manifest");

        let report = run_manifest(&manifest_path, None, false, false, None).expect("run benchmark");
        fs::remove_dir_all(directory).expect("remove fixtures");

        assert_eq!(report.summary.analyzed_tracks, 1);
        assert_eq!(report.summary.failed_tracks, 0);
        assert!(report.tracks[0].error.is_none());
        assert!(report.tracks[0].bpm_absolute_error.is_some());
    }
}
