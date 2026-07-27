use std::{env, error::Error, path::Path, process::ExitCode, time::Instant};

use serde::Serialize;
use trailmix::{Analysis, AnalysisConfig, AudioBuffer, Mode, MusicalKey, PitchClass};
use trailmix_manifest::{KeySegmentAnnotation, TempoSegmentAnnotation, TrackAnnotation};

const SAMPLE_RATE: u32 = 44_100;
const DURATION_SECONDS: u32 = 20;

#[derive(Serialize)]
struct SyntheticBenchmark {
    version: u32,
    sample_rate: u32,
    duration_seconds: u32,
    cases: Vec<SyntheticCase>,
}

#[derive(Serialize)]
struct SyntheticCase {
    expected_bpm: f32,
    detected_bpm: Option<f32>,
    absolute_error: Option<f32>,
    confidence: f32,
    elapsed_milliseconds: f64,
}

#[derive(Serialize)]
struct CorpusBenchmark {
    version: u32,
    manifest_version: u32,
    track_count: usize,
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
    tempo_segment_mean_absolute_error: Option<f32>,
    key_segment_exact_accuracy: Option<f32>,
    mean_decode_milliseconds: Option<f64>,
    mean_analysis_milliseconds: Option<f64>,
}

#[derive(Serialize)]
struct TrackResult {
    id: String,
    split: Option<String>,
    duration_seconds: Option<f64>,
    decode_milliseconds: Option<f64>,
    analysis_milliseconds: Option<f64>,
    expected_bpm: Option<f32>,
    detected_bpm: Option<f32>,
    bpm_absolute_error: Option<f32>,
    bpm_octave_aware_absolute_error: Option<f32>,
    expected_key: Option<String>,
    detected_key: Option<String>,
    exact_key_match: Option<bool>,
    tempo_segment_mean_absolute_error: Option<f32>,
    key_segment_exact_accuracy: Option<f32>,
    error: Option<String>,
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
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let result = match arguments.next() {
        None => serde_json::to_string_pretty(&run_synthetic())?,
        Some(flag) if flag == "--manifest" => {
            let path = arguments
                .next()
                .ok_or("usage: trailmix-bench [--manifest <manifest.json>]")?;
            if arguments.next().is_some() {
                return Err("usage: trailmix-bench [--manifest <manifest.json>]".into());
            }
            serde_json::to_string_pretty(&run_manifest(Path::new(&path))?)?
        }
        Some(_) => return Err("usage: trailmix-bench [--manifest <manifest.json>]".into()),
    };
    println!("{result}");
    Ok(())
}

fn run_synthetic() -> SyntheticBenchmark {
    let cases = [90.0, 120.0, 128.0].into_iter().map(run_case).collect();
    SyntheticBenchmark {
        version: 1,
        sample_rate: SAMPLE_RATE,
        duration_seconds: DURATION_SECONDS,
        cases,
    }
}

fn run_case(expected_bpm: f32) -> SyntheticCase {
    let samples = synthetic_track(expected_bpm);
    let started = Instant::now();
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &samples,
            sample_rate: SAMPLE_RATE,
        },
        AnalysisConfig::default(),
    );
    let elapsed_milliseconds = started.elapsed().as_secs_f64() * 1_000.0;

    SyntheticCase {
        expected_bpm,
        detected_bpm: analysis.beat.global_bpm,
        absolute_error: analysis
            .beat
            .global_bpm
            .map(|detected| (detected - expected_bpm).abs()),
        confidence: analysis.beat.confidence,
        elapsed_milliseconds,
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

fn run_manifest(path: &Path) -> Result<CorpusBenchmark, Box<dyn Error>> {
    let manifest = trailmix_manifest::load(path)?;
    let base_directory = path.parent().unwrap_or_else(|| Path::new("."));
    let tracks = manifest
        .tracks
        .iter()
        .map(|track| analyze_manifest_track(track, base_directory))
        .collect::<Vec<_>>();
    let summary = summarize(&tracks);

    Ok(CorpusBenchmark {
        version: 1,
        manifest_version: manifest.version,
        track_count: tracks.len(),
        summary,
        tracks,
    })
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
    let analysis_started = Instant::now();
    let analysis = trailmix::analyze(
        AudioBuffer {
            samples: &decoded.samples,
            sample_rate: decoded.sample_rate,
        },
        AnalysisConfig::default(),
    );
    let analysis_milliseconds = analysis_started.elapsed().as_secs_f64() * 1_000.0;

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

    TrackResult {
        id: track.id.clone(),
        split: track.split.clone(),
        duration_seconds: Some(decoded.duration_seconds()),
        decode_milliseconds: Some(decode_milliseconds),
        analysis_milliseconds: Some(analysis_milliseconds),
        expected_bpm: track.expected_bpm,
        detected_bpm: analysis.beat.global_bpm,
        bpm_absolute_error,
        bpm_octave_aware_absolute_error,
        expected_key: expected_key.map(|key| key.to_string()),
        detected_key: detected_key.map(|key| key.to_string()),
        exact_key_match: expected_key.map(|expected| Some(expected) == detected_key),
        tempo_segment_mean_absolute_error: tempo_segment_error(
            &track.expected_tempo_segments,
            &analysis,
        ),
        key_segment_exact_accuracy,
        error: None,
    }
}

fn failed_track(track: &TrackAnnotation, error: String) -> TrackResult {
    TrackResult {
        id: track.id.clone(),
        split: track.split.clone(),
        duration_seconds: None,
        decode_milliseconds: None,
        analysis_milliseconds: None,
        expected_bpm: track.expected_bpm,
        detected_bpm: None,
        bpm_absolute_error: None,
        bpm_octave_aware_absolute_error: None,
        expected_key: track.expected_key.clone(),
        detected_key: None,
        exact_key_match: None,
        tempo_segment_mean_absolute_error: None,
        key_segment_exact_accuracy: None,
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
        tempo_segment_mean_absolute_error: mean_f32(&segment_errors),
        key_segment_exact_accuracy: mean_f32(&key_segment_accuracies),
        mean_decode_milliseconds: mean_f64(&decode_times),
        mean_analysis_milliseconds: mean_f64(&analysis_times),
    }
}

fn mean_f32(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

fn mean_f64(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn parse_key(value: &str) -> Result<MusicalKey, String> {
    let normalized = value.trim().replace('♯', "#").replace('♭', "b");
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
        "minor" | "min" => Mode::Minor,
        _ => return Err(format!("invalid expected key mode: {mode}")),
    };
    Ok(MusicalKey { tonic, mode })
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
    fn treats_half_time_as_octave_equivalent() {
        assert!((octave_aware_error(128.0, 64.0)).abs() < f32::EPSILON);
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

        let report = run_manifest(&manifest_path).expect("run benchmark");
        fs::remove_dir_all(directory).expect("remove fixtures");

        assert_eq!(report.summary.analyzed_tracks, 1);
        assert_eq!(report.summary.failed_tracks, 0);
        assert!(report.tracks[0].error.is_none());
        assert!(report.tracks[0].bpm_absolute_error.is_some());
    }
}
