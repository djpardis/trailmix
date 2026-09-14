//! First-party model backend contracts for trail mix.
//!
//! This crate is the home for learned backend work that should live beside the
//! default DSP path. It intentionally contains no model weights, converted
//! artifacts, or third-party inference runtime. Concrete backends can be added
//! here after their provenance, licenses, artifact hashes, runtime cost, and
//! output mapping have been reviewed.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use trailmix::{Analysis, AnalysisConfig, AudioBuffer};

/// Review state for a learned backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelBackendStatus {
    Planned,
    Experimental,
    Evaluated,
}

/// Artifact metadata required before a model result can be reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelArtifact {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub code_license: String,
    pub weight_license: String,
    pub training_disclosure: String,
}

/// Runtime and packaging evidence recorded for a backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelBackendEvidence {
    pub mean_analysis_milliseconds: f64,
    pub median_analysis_milliseconds: f64,
    pub runtime_dependency_size_bytes: u64,
    pub output_json_size_bytes: u64,
    pub hardware_target: String,
}

/// Metadata exposed by every learned backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelBackendManifest {
    pub name: String,
    pub status: ModelBackendStatus,
    pub artifact: ModelArtifact,
    pub evidence: Option<ModelBackendEvidence>,
}

/// A learned backend that maps audio into the stable trail mix result shape.
pub trait ModelBackend {
    /// Static backend metadata used for benchmark reports and integration review.
    fn manifest(&self) -> &ModelBackendManifest;

    /// Analyze mono PCM and return the same `analysis-v1` shape as the DSP path.
    ///
    /// Implementations may fill only the components they own, but they must keep
    /// the serialized `Analysis` contract compatible with application storage.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend artifact, runtime, input, or inference
    /// result cannot produce a valid `Analysis`.
    fn analyze(
        &self,
        audio: AudioBuffer<'_>,
        config: AnalysisConfig,
    ) -> Result<Analysis, ModelBackendError>;
}

/// Errors returned by learned backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelBackendError {
    Artifact(String),
    Runtime(String),
    Inference(String),
    OutputMapping(String),
}

impl fmt::Display for ModelBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Artifact(message) => write!(formatter, "model artifact error: {message}"),
            Self::Runtime(message) => write!(formatter, "model runtime error: {message}"),
            Self::Inference(message) => write!(formatter, "model inference error: {message}"),
            Self::OutputMapping(message) => {
                write!(formatter, "model output mapping error: {message}")
            }
        }
    }
}

impl Error for ModelBackendError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_as_snake_case() {
        let encoded = serde_json::to_string(&ModelBackendStatus::Experimental).expect("serialize");
        assert_eq!(encoded, "\"experimental\"");
    }
}
