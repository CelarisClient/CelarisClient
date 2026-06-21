//! Structured, per-stage error model for the launch pipeline.
//!
//! Every stage returns [`StageResult`]; failures carry an explicit [`Stage`] and
//! [`ErrorCode`] so callers and tests can assert on *what* failed deterministically
//! instead of grepping raw log text.

use std::fmt;

use serde::Serialize;

/// The four contractual stages of a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Stage {
    Resolve,
    Download,
    Inject,
    Launch,
}

/// Specific, stable failure reasons. Grouped by the stage that raises them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    // --- resolve ---
    ManifestUnreachable,
    ManifestInvalid,
    VersionNotFound,
    /// Downloaded version JSON did not match the SHA1 in the manifest.
    VersionHashMismatch,
    VersionJsonInvalid,
    AssetIndexUnreachable,
    AssetIndexInvalid,
    FabricMetaUnreachable,
    FabricMetaInvalid,

    // --- download ---
    DownloadFailed,
    Sha1Mismatch,

    // --- inject ---
    NativesExtractFailed,
    FabricLoaderMissing,
    FabricApiMissing,
    /// An expected (resolved) mod jar is not present in `mods/` after injection.
    ModMissing,

    // --- launch ---
    GameDirError,
    SpawnFailed,
    ProcessExitedEarly,
}

/// A failure tagged with the stage and code that produced it.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchError {
    pub stage: Stage,
    pub code: ErrorCode,
    pub message: String,
}

impl LaunchError {
    pub fn new(stage: Stage, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            stage,
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}/{:?}] {}", self.stage, self.code, self.message)
    }
}

impl std::error::Error for LaunchError {}

pub type StageResult<T> = Result<T, LaunchError>;
