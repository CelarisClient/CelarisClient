//! Deterministic Minecraft launch pipeline.
//!
//! Tauri-free on purpose so it can be unit/contract-tested without the WebView
//! toolchain. Four contractual stages, each with explicit validation and
//! structured [`error::ErrorCode`]s:
//!   * [`resolver`] / `runner::resolve_stage` – version manifest integrity → plan
//!   * `runner::download_stage` – SHA1-validated artifact downloads
//!   * `runner::inject_stage` – Fabric + Celaris mod presence
//!   * `runner::launch_stage` – process startup success
//!
//! Progress + log lines are reported through [`Reporter`]; failures are returned
//! as structured [`error::LaunchError`]s rather than raw log strings.

pub mod auth;
pub mod download;
pub mod error;
pub mod mods;
pub mod resolver;
pub mod runner;

#[cfg(test)]
mod contract_tests;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

// Re-exported for ergonomic access; the full sets live in their modules.
pub use auth::Session;
pub use error::StageResult;

/// Coarse-grained progress update for the UI.
#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub message: String,
    pub current: u64,
    pub total: u64,
}

/// Sink for progress + log output. Implemented by the Tauri layer.
pub trait Reporter: Send + Sync {
    fn progress(&self, progress: Progress);
    fn log(&self, line: &str);
}

/// The formal input contract for a launch: everything needed to produce a
/// deterministic Minecraft + Fabric + Celaris startup.
///
/// This is the boundary the new layers feed into — the auth layer supplies the
/// [`Session`], the mod-resolution layer supplies the ordered [`Self::mods`] —
/// while the launch engine only consumes this struct.
#[derive(Clone)]
pub struct CelarisLaunchConfig {
    /// Vanilla Minecraft version id, e.g. "1.21.11".
    pub mc_version: String,
    /// Absolute path to the `java` executable.
    pub java_path: String,
    /// Max heap in megabytes (-Xmx).
    pub max_ram_mb: u32,
    /// Validated player session (produced by the auth layer).
    pub session: Session,
    /// Per-instance game directory (`.minecraft`).
    pub game_dir: PathBuf,
    /// Shared launcher root holding versions/libraries/assets/natives.
    pub root_dir: PathBuf,
    /// Inject the Fabric loader (required for the Celaris client).
    pub use_fabric: bool,
    /// Resolved, deterministically ordered mod jars (produced by the mod layer)
    /// to drop into `mods/` before launch.
    pub mods: Vec<PathBuf>,
    /// Extra JVM arguments (after -Xmx), e.g. `-XX:+UseG1GC`.
    pub extra_jvm_args: Vec<String>,
    /// Extra environment variables for the game process.
    pub env: Vec<(String, String)>,
    /// If set, joins this server address directly on launch
    /// (`--quickPlayMultiplayer <addr>`).
    pub quick_play_multiplayer: Option<String>,
}

/// Runs the full pipeline: resolve → download → inject → launch.
pub async fn install_and_launch(
    config: CelarisLaunchConfig,
    reporter: Arc<dyn Reporter>,
) -> StageResult<()> {
    runner::install_and_launch(config, reporter).await
}
