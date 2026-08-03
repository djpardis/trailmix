use trailmix::{
    Analysis, BeatAnalysis, BeatPosition, KeyAnalysis, KeySegment, Mode, MusicalKey, PitchClass,
    TempoSegment, WaveformColumn, WaveformOverview,
};

#[test]
fn analysis_v1_json_matches_golden_fixture() {
    let analysis = Analysis {
        version: 1,
        duration_seconds: 12.5,
        beat: BeatAnalysis {
            version: 3,
            global_bpm: Some(124.0),
            confidence: 0.75,
            beats: vec![
                BeatPosition {
                    time_seconds: 0.5,
                    confidence: 0.75,
                    position_in_bar: 1,
                },
                BeatPosition {
                    time_seconds: 0.984,
                    confidence: 0.5,
                    position_in_bar: 2,
                },
            ],
            tempo_segments: vec![TempoSegment {
                start_seconds: 0.0,
                end_seconds: 12.5,
                bpm: 124.0,
                confidence: 0.75,
            }],
            multi_tempo: false,
            alternate_bpm: None,
            alternate_coverage: 0.0,
        },
        key: KeyAnalysis {
            version: 6,
            key: Some(MusicalKey {
                tonic: PitchClass::A,
                mode: Mode::Minor,
            }),
            confidence: 0.5,
            chroma: [0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.0],
            segments: vec![KeySegment {
                start_seconds: 0.0,
                end_seconds: 12.5,
                key: MusicalKey {
                    tonic: PitchClass::A,
                    mode: Mode::Minor,
                },
                confidence: 0.5,
            }],
            multi_key: false,
            alternate_key: None,
            alternate_coverage: 0.0,
        },
        waveform: WaveformOverview {
            version: 1,
            sample_rate: 48_000,
            source_samples: 600_000,
            columns: vec![
                WaveformColumn {
                    min: -1.0,
                    max: 1.0,
                    rms: 0.5,
                },
                WaveformColumn {
                    min: -0.25,
                    max: 0.75,
                    rms: 0.25,
                },
            ],
        },
    };

    let actual = serde_json::to_string_pretty(&analysis).expect("analysis serializes to JSON");
    let expected = include_str!("fixtures/analysis-v1.json")
        .replace("\r\n", "\n")
        .trim_end()
        .to_owned();

    assert_eq!(actual, expected);
}
