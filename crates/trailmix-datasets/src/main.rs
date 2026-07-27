use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use trailmix_manifest::{
    AnnotationManifest, AnnotationMetadata, AnnotationStatus, DatasetReference, TrackAnnotation,
};

const TEMPO_CITATION: &str = "Knees et al., Two Data Sets for Tempo Estimation and Key \
    Detection in Electronic Dance Music Annotated from User Corrections, ISMIR 2015; \
    Schreiber and Müller, A Crowdsourced Experiment for Tempo Estimation of Electronic \
    Dance Music, ISMIR 2018.";
const KEY_CITATION: &str = "Knees et al., Two Data Sets for Tempo Estimation and Key \
    Detection in Electronic Dance Music Annotated from User Corrections, ISMIR 2015.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatasetKind {
    Tempo,
    Key,
}

impl DatasetKind {
    fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "giantsteps-tempo" => Some(Self::Tempo),
            "giantsteps-key" => Some(Self::Key),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Tempo => "GiantSteps Tempo",
            Self::Key => "GiantSteps Key",
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Tempo => "v2",
            Self::Key => "2015",
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::Tempo => "giantsteps-tempo",
            Self::Key => "giantsteps-key",
        }
    }

    fn citation(self) -> &'static str {
        match self {
            Self::Tempo => TEMPO_CITATION,
            Self::Key => KEY_CITATION,
        }
    }
}

#[derive(Debug)]
struct ImportSummary {
    imported: usize,
    updated: usize,
    skipped: usize,
    missing_audio: usize,
}

enum ReferenceValue {
    Tempo(f32),
    Key(String),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("trailmix-datasets: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let kind = arguments
        .next()
        .as_deref()
        .and_then(DatasetKind::parse)
        .ok_or_else(usage)?;
    let dataset_root = arguments.next().ok_or_else(usage)?;
    let audio_root = arguments.next().ok_or_else(usage)?;
    let manifest_path = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage().into());
    }

    let summary = import(
        kind,
        Path::new(&dataset_root),
        Path::new(&audio_root),
        Path::new(&manifest_path),
    )?;
    println!(
        "{}: {} added, {} updated, {} skipped, {} audio files missing",
        kind.name(),
        summary.imported,
        summary.updated,
        summary.skipped,
        summary.missing_audio
    );
    Ok(())
}

fn usage() -> String {
    "usage: trailmix-datasets <giantsteps-tempo|giantsteps-key> \
     <dataset-root> <audio-root> <manifest.json>"
        .to_owned()
}

fn import(
    kind: DatasetKind,
    dataset_root: &Path,
    audio_root: &Path,
    manifest_path: &Path,
) -> Result<ImportSummary, Box<dyn Error>> {
    let annotation_directory = annotation_directory(kind, dataset_root)?;
    let extension = match kind {
        DatasetKind::Tempo => "bpm",
        DatasetKind::Key => "key",
    };
    let mut annotation_paths = fs::read_dir(&annotation_directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some(extension))
        .collect::<Vec<_>>();
    annotation_paths.sort();

    let mut manifest = if manifest_path.exists() {
        trailmix_manifest::load(manifest_path)?
    } else {
        AnnotationManifest::default()
    };
    let manifest_directory = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut summary = ImportSummary {
        imported: 0,
        updated: 0,
        skipped: 0,
        missing_audio: 0,
    };

    for annotation_path in annotation_paths {
        let item_id = annotation_path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or("annotation filename is not valid UTF-8")?;
        let contents = fs::read_to_string(&annotation_path)?;
        let Some(reference) = parse_annotation(kind, &contents)? else {
            summary.skipped += 1;
            continue;
        };
        let audio_path = audio_root.join(format!("{item_id}.mp3"));
        if !audio_path.exists() {
            summary.missing_audio += 1;
        }
        let stored_path =
            pathdiff::diff_paths(&audio_path, manifest_directory).unwrap_or(audio_path.clone());
        let id = format!("{}-{item_id}", kind.id_prefix());
        let source = DatasetReference {
            name: kind.name().to_owned(),
            version: kind.version().to_owned(),
            item_id: item_id.to_owned(),
            citation: Some(kind.citation().to_owned()),
        };

        if let Some(existing) = manifest.tracks.iter_mut().find(|track| track.id == id) {
            existing.path = stored_path;
            existing.source = Some(source);
            existing
                .split
                .get_or_insert_with(|| "evaluation".to_owned());
            apply_annotation(existing, reference);
            summary.updated += 1;
        } else {
            let mut track = TrackAnnotation {
                id,
                path: stored_path,
                split: Some("evaluation".to_owned()),
                source: Some(source),
                expected_bpm: None,
                expected_key: None,
                expected_beats: Vec::new(),
                expected_tempo_segments: Vec::new(),
                expected_key_segments: Vec::new(),
                change_events: Vec::new(),
                serato: None,
                annotation: AnnotationMetadata {
                    status: AnnotationStatus::Reviewed,
                    reviewer: Some(kind.name().to_owned()),
                    confidence: None,
                    notes: Some("Imported reference annotation".to_owned()),
                },
            };
            apply_annotation(&mut track, reference);
            manifest.tracks.push(track);
            summary.imported += 1;
        }
    }

    manifest
        .tracks
        .sort_by(|left, right| left.id.cmp(&right.id));
    trailmix_manifest::save(manifest_path, &manifest)?;
    Ok(summary)
}

fn annotation_directory(kind: DatasetKind, root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let candidates = match kind {
        DatasetKind::Tempo => [
            root.join("annotations_v2/tempo"),
            root.join("annotations/tempo"),
            root.join("annotations/giantsteps"),
        ],
        DatasetKind::Key => [
            root.join("annotations/giantsteps"),
            root.join("annotations/key"),
            root.join("annotations/jams"),
        ],
    };
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .ok_or_else(|| {
            format!(
                "could not find {} annotations under {}",
                kind.name(),
                root.display()
            )
            .into()
        })
}

fn parse_annotation(
    kind: DatasetKind,
    contents: &str,
) -> Result<Option<ReferenceValue>, Box<dyn Error>> {
    match kind {
        DatasetKind::Tempo => {
            let value = contents
                .lines()
                .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .ok_or("tempo annotation is empty")?
                .trim()
                .parse::<f32>()?;
            if !value.is_finite() || value <= 0.0 {
                return Ok(None);
            }
            Ok(Some(ReferenceValue::Tempo(value)))
        }
        DatasetKind::Key => {
            let line = contents
                .lines()
                .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .ok_or("key annotation is empty")?;
            let mut fields = line.split_whitespace();
            if fields.next() != Some("key") {
                return Err(format!("unsupported key annotation: {line}").into());
            }
            let _timestamp = fields.next().ok_or("key annotation has no timestamp")?;
            let key = fields.collect::<Vec<_>>().join(" ");
            if key.is_empty() {
                return Err("key annotation has no key value".into());
            }
            Ok(Some(ReferenceValue::Key(key)))
        }
    }
}

fn apply_annotation(track: &mut TrackAnnotation, reference: ReferenceValue) {
    match reference {
        ReferenceValue::Tempo(value) => track.expected_bpm = Some(value),
        ReferenceValue::Key(value) => track.expected_key = Some(value),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn fixture_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        env::temp_dir().join(format!("trailmix-{label}-{nonce}"))
    }

    #[test]
    fn imports_tempo_v2_and_is_idempotent() {
        let directory = fixture_directory("tempo-import");
        let dataset = directory.join("dataset");
        let audio = directory.join("audio");
        let manifest = directory.join("manifest.json");
        fs::create_dir_all(dataset.join("annotations_v2/tempo")).expect("annotation directory");
        fs::create_dir_all(&audio).expect("audio directory");
        fs::write(
            dataset.join("annotations_v2/tempo/1030011.LOFI.bpm"),
            "127.0\n",
        )
        .expect("tempo annotation");
        fs::write(audio.join("1030011.LOFI.mp3"), []).expect("audio fixture");

        let first = import(DatasetKind::Tempo, &dataset, &audio, &manifest).expect("first import");
        let second =
            import(DatasetKind::Tempo, &dataset, &audio, &manifest).expect("second import");
        let imported = trailmix_manifest::load(&manifest).expect("manifest");
        fs::remove_dir_all(directory).expect("remove fixture");

        assert_eq!(first.imported, 1);
        assert_eq!(first.missing_audio, 0);
        assert_eq!(second.imported, 0);
        assert_eq!(second.updated, 1);
        assert_eq!(imported.tracks.len(), 1);
        assert_eq!(imported.tracks[0].expected_bpm, Some(127.0));
    }

    #[test]
    fn imports_key_and_reports_missing_audio() {
        let directory = fixture_directory("key-import");
        let dataset = directory.join("dataset");
        let audio = directory.join("audio");
        let manifest = directory.join("manifest.json");
        fs::create_dir_all(dataset.join("annotations/giantsteps")).expect("annotation directory");
        fs::create_dir_all(&audio).expect("audio directory");
        fs::write(
            dataset.join("annotations/giantsteps/1004923.LOFI.key"),
            "#@format: key timestamp key\nkey 0 C minor\n",
        )
        .expect("key annotation");

        let summary = import(DatasetKind::Key, &dataset, &audio, &manifest).expect("key import");
        let imported = trailmix_manifest::load(&manifest).expect("manifest");
        fs::remove_dir_all(directory).expect("remove fixture");

        assert_eq!(summary.imported, 1);
        assert_eq!(summary.missing_audio, 1);
        assert_eq!(imported.tracks[0].expected_key.as_deref(), Some("C minor"));
    }
}
