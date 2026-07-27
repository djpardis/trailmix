//! Versioned private-corpus annotations shared by Test Kitchen and benchmarks.

use std::{
    collections::HashSet,
    error::Error as StdError,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const CURRENT_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationManifest {
    pub version: u32,
    pub tracks: Vec<TrackAnnotation>,
}

impl Default for AnnotationManifest {
    fn default() -> Self {
        Self {
            version: CURRENT_MANIFEST_VERSION,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackAnnotation {
    pub id: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<DatasetReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_bpm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_beats: Vec<BeatAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_tempo_segments: Vec<TempoSegmentAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_key_segments: Vec<KeySegmentAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_events: Vec<ChangeEventAnnotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serato: Option<SeratoObservation>,
    #[serde(default)]
    pub annotation: AnnotationMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetReference {
    pub name: String,
    pub version: String,
    pub item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeatAnnotation {
    pub time_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_in_bar: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoSegmentAnnotation {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub bpm: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_bpm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeySegmentAnnotation {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeEventAnnotation {
    pub time_seconds: f64,
    #[serde(default)]
    pub structure_changed: bool,
    #[serde(default)]
    pub tempo_changed: bool,
    #[serde(default)]
    pub key_changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SeratoObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub beat_grid_seconds: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_user_edited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_user_edited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationMetadata {
    #[serde(default)]
    pub status: AnnotationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Default for AnnotationMetadata {
    fn default() -> Self {
        Self {
            status: AnnotationStatus::Draft,
            reviewer: None,
            confidence: None,
            notes: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationStatus {
    #[default]
    Draft,
    Reviewed,
    Adjudicated,
}

#[derive(Debug)]
pub enum ManifestError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Invalid(String),
    Serialize(serde_json::Error),
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "could not parse {}: {source}", path.display())
            }
            Self::Invalid(message) => write!(formatter, "invalid manifest: {message}"),
            Self::Serialize(source) => write!(formatter, "could not serialize manifest: {source}"),
            Self::Write { path, source } => {
                write!(formatter, "could not write {}: {source}", path.display())
            }
        }
    }
}

impl StdError for ManifestError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } | Self::Serialize(source) => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

/// Load and validate an annotation manifest.
///
/// # Errors
///
/// Returns [`ManifestError`] when the file cannot be read, parsed, or
/// validated.
pub fn load(path: impl AsRef<Path>) -> Result<AnnotationManifest, ManifestError> {
    let path = path.as_ref();
    let contents = fs::read(path).map_err(|source| ManifestError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest = serde_json::from_slice(&contents).map_err(|source| ManifestError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    validate(&manifest)?;
    Ok(manifest)
}

/// Validate IDs, ranges, confidence values, and timeline ordering.
///
/// # Errors
///
/// Returns [`ManifestError::Invalid`] for invalid annotations.
pub fn validate(manifest: &AnnotationManifest) -> Result<(), ManifestError> {
    if manifest.version != CURRENT_MANIFEST_VERSION {
        return Err(ManifestError::Invalid(format!(
            "unsupported version {}, expected {CURRENT_MANIFEST_VERSION}",
            manifest.version
        )));
    }

    let mut ids = HashSet::new();
    for track in &manifest.tracks {
        if track.id.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "track ID cannot be empty".to_owned(),
            ));
        }
        if !ids.insert(track.id.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "duplicate track ID {}",
                track.id
            )));
        }
        validate_track(track)?;
    }
    Ok(())
}

/// Save a validated manifest through a temporary file in the same directory.
///
/// # Errors
///
/// Returns [`ManifestError`] when validation, serialization, or writing fails.
pub fn save(path: impl AsRef<Path>, manifest: &AnnotationManifest) -> Result<(), ManifestError> {
    validate(manifest)?;
    let path = path.as_ref();
    let json = serde_json::to_vec_pretty(manifest).map_err(ManifestError::Serialize)?;
    let temporary_path = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary_path, json).map_err(|source| ManifestError::Write {
        path: temporary_path.clone(),
        source,
    })?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|source| ManifestError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&temporary_path, path).map_err(|source| ManifestError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_track(track: &TrackAnnotation) -> Result<(), ManifestError> {
    if track.path.as_os_str().is_empty() {
        return Err(invalid_track(track, "audio path cannot be empty"));
    }
    if let Some(bpm) = track.expected_bpm {
        validate_bpm(track, bpm)?;
    }
    validate_confidence(track, track.annotation.confidence)?;
    validate_ordered_times(
        track,
        track.expected_beats.iter().map(|beat| beat.time_seconds),
        "beat",
    )?;
    validate_ranges(
        track,
        track.expected_tempo_segments.iter().map(|segment| {
            (
                segment.start_seconds,
                segment.end_seconds,
                segment.confidence,
            )
        }),
        "tempo",
    )?;
    for segment in &track.expected_tempo_segments {
        validate_bpm(track, segment.bpm)?;
        if let Some(end_bpm) = segment.end_bpm {
            validate_bpm(track, end_bpm)?;
        }
    }
    validate_ranges(
        track,
        track.expected_key_segments.iter().map(|segment| {
            (
                segment.start_seconds,
                segment.end_seconds,
                segment.confidence,
            )
        }),
        "key",
    )?;
    if track
        .expected_key_segments
        .iter()
        .any(|segment| segment.key.trim().is_empty())
    {
        return Err(invalid_track(track, "key segment cannot have an empty key"));
    }
    validate_ordered_times(
        track,
        track.change_events.iter().map(|event| event.time_seconds),
        "change event",
    )?;
    for event in &track.change_events {
        validate_confidence(track, event.confidence)?;
        if !event.structure_changed && !event.tempo_changed && !event.key_changed {
            return Err(invalid_track(
                track,
                "change event must select at least one change type",
            ));
        }
    }
    if let Some(serato) = &track.serato {
        if let Some(bpm) = serato.bpm {
            validate_bpm(track, bpm)?;
        }
        validate_ordered_times(
            track,
            serato.beat_grid_seconds.iter().copied(),
            "Serato beat",
        )?;
    }
    Ok(())
}

fn validate_bpm(track: &TrackAnnotation, bpm: f32) -> Result<(), ManifestError> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err(invalid_track(track, "BPM must be finite and positive"));
    }
    Ok(())
}

fn validate_confidence(
    track: &TrackAnnotation,
    confidence: Option<f32>,
) -> Result<(), ManifestError> {
    if confidence.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(invalid_track(track, "confidence must be between 0 and 1"));
    }
    Ok(())
}

fn validate_ordered_times(
    track: &TrackAnnotation,
    times: impl Iterator<Item = f64>,
    label: &str,
) -> Result<(), ManifestError> {
    let mut previous = None;
    for time in times {
        if !time.is_finite() || time < 0.0 {
            return Err(invalid_track(
                track,
                &format!("{label} time must be finite and non-negative"),
            ));
        }
        if previous.is_some_and(|value| time < value) {
            return Err(invalid_track(
                track,
                &format!("{label} times must be ordered"),
            ));
        }
        previous = Some(time);
    }
    Ok(())
}

fn validate_ranges(
    track: &TrackAnnotation,
    ranges: impl Iterator<Item = (f64, f64, Option<f32>)>,
    label: &str,
) -> Result<(), ManifestError> {
    let mut previous_end = None;
    for (start, end, confidence) in ranges {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            return Err(invalid_track(
                track,
                &format!("{label} segment must have a valid positive range"),
            ));
        }
        if previous_end.is_some_and(|value| start < value) {
            return Err(invalid_track(
                track,
                &format!("{label} segments cannot overlap"),
            ));
        }
        validate_confidence(track, confidence)?;
        previous_end = Some(end);
    }
    Ok(())
}

fn invalid_track(track: &TrackAnnotation, message: &str) -> ManifestError {
    ManifestError::Invalid(format!("track {}: {message}", track.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> TrackAnnotation {
        TrackAnnotation {
            id: "example".to_owned(),
            path: PathBuf::from("local/example.wav"),
            split: Some("evaluation".to_owned()),
            source: None,
            expected_bpm: Some(120.0),
            expected_key: Some("A minor".to_owned()),
            expected_beats: Vec::new(),
            expected_tempo_segments: vec![TempoSegmentAnnotation {
                start_seconds: 0.0,
                end_seconds: 10.0,
                bpm: 120.0,
                end_bpm: None,
                confidence: Some(1.0),
            }],
            expected_key_segments: Vec::new(),
            change_events: Vec::new(),
            serato: None,
            annotation: AnnotationMetadata::default(),
        }
    }

    #[test]
    fn accepts_a_valid_manifest() {
        let manifest = AnnotationManifest {
            version: CURRENT_MANIFEST_VERSION,
            tracks: vec![track()],
        };
        assert!(validate(&manifest).is_ok());
    }

    #[test]
    fn rejects_an_empty_change_event() {
        let mut track = track();
        track.change_events.push(ChangeEventAnnotation {
            time_seconds: 5.0,
            structure_changed: false,
            tempo_changed: false,
            key_changed: false,
            label: None,
            confidence: None,
        });
        let error = validate(&AnnotationManifest {
            version: CURRENT_MANIFEST_VERSION,
            tracks: vec![track],
        })
        .expect_err("invalid event");
        assert!(error.to_string().contains("at least one"));
    }

    #[test]
    fn rejects_overlapping_segments() {
        let mut track = track();
        track.expected_tempo_segments.push(TempoSegmentAnnotation {
            start_seconds: 9.0,
            end_seconds: 12.0,
            bpm: 128.0,
            end_bpm: None,
            confidence: None,
        });
        assert!(
            validate(&AnnotationManifest {
                version: CURRENT_MANIFEST_VERSION,
                tracks: vec![track],
            })
            .is_err()
        );
    }
}
