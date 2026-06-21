//! Deterministic launch pipeline: resolve → download → inject → launch.
//!
//! Each stage has an explicit contract:
//!   * **resolve** validates version-manifest integrity (version JSON SHA1) and
//!     produces a complete [`LaunchPlan`] (every artifact to fetch + the computed
//!     classpath / main class / launch args).
//!   * **download** fetches every planned artifact and validates SHA1 correctness.
//!   * **inject** extracts natives and places the Fabric API + Celaris mod, then
//!     validates their presence.
//!   * **launch** builds the JVM command, spawns it and validates process startup.
//!
//! Failures surface as structured [`LaunchError`]s (stage + code), never raw logs.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use super::download::{self, DownloadError, DownloadItem};
use super::error::{ErrorCode, LaunchError, Stage, StageResult};
use super::resolver::{self, SelectError, VersionJson};
use super::{CelarisLaunchConfig, Progress, Reporter};

/// How long to watch a freshly spawned process before declaring startup a success.
const STARTUP_GRACE: Duration = Duration::from_millis(300);

/// Fully resolved, deterministic description of what a launch will do.
pub struct LaunchPlan {
    pub version: VersionJson,
    pub main_class: String,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_index_id: String,
    pub downloads: Vec<DownloadItem>,
    /// Fabric loader jars (subset of classpath) validated during inject.
    pub fabric_libs: Vec<PathBuf>,
    /// Expected Fabric API jar in `mods/`, when the Celaris client is requested.
    pub fabric_api_jar: Option<PathBuf>,
}

/// Runs the whole pipeline.
pub async fn install_and_launch(
    config: CelarisLaunchConfig,
    reporter: Arc<dyn Reporter>,
) -> StageResult<()> {
    let client = download::client()
        .map_err(|e| LaunchError::new(Stage::Resolve, ErrorCode::ManifestUnreachable, e.to_string()))?;
    let r = reporter.as_ref();

    let plan = resolve_stage(&config, &client, r).await?;
    download_stage(&plan, &client, r).await?;
    inject_stage(&config, &plan, r)?;
    launch_stage(&config, &plan, reporter.clone())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Stage 1: resolve
// ---------------------------------------------------------------------------

async fn resolve_stage(
    config: &CelarisLaunchConfig,
    client: &reqwest::Client,
    reporter: &dyn Reporter,
) -> StageResult<LaunchPlan> {
    announce(reporter, "resolve", "Auflösen der Version…");

    let libraries_dir = config.root_dir.join("libraries");
    let assets_dir = config.root_dir.join("assets");
    let natives_dir = config.root_dir.join("natives").join(&config.mc_version);
    let versions_dir = config.root_dir.join("versions");

    // Manifest → version reference.
    let manifest_text = download::get_text(client, resolver::VERSION_MANIFEST)
        .await
        .map_err(|e| LaunchError::new(Stage::Resolve, ErrorCode::ManifestUnreachable, e.to_string()))?;
    let version_ref = resolver::select_version(&manifest_text, &config.mc_version).map_err(|e| match e {
        SelectError::Invalid(m) => LaunchError::new(Stage::Resolve, ErrorCode::ManifestInvalid, m),
        SelectError::NotFound(m) => {
            LaunchError::new(Stage::Resolve, ErrorCode::VersionNotFound, format!("unknown version: {m}"))
        }
    })?;

    // Integrity: the downloaded version JSON must match the manifest's SHA1.
    let version_bytes = download::get_bytes(client, &version_ref.url)
        .await
        .map_err(|e| LaunchError::new(Stage::Resolve, ErrorCode::ManifestUnreachable, e.to_string()))?;
    let got = download::sha1_hex(&version_bytes);
    if got != version_ref.sha1 {
        return Err(LaunchError::new(
            Stage::Resolve,
            ErrorCode::VersionHashMismatch,
            format!("version JSON sha1 {got} != manifest {}", version_ref.sha1),
        ));
    }
    let version_text = String::from_utf8(version_bytes)
        .map_err(|e| LaunchError::new(Stage::Resolve, ErrorCode::VersionJsonInvalid, e.to_string()))?;
    let version = resolver::parse_version_json(&version_text)
        .map_err(|m| LaunchError::new(Stage::Resolve, ErrorCode::VersionJsonInvalid, m))?;

    // Build the download set.
    let client_jar = versions_dir
        .join(&config.mc_version)
        .join(format!("{}.jar", config.mc_version));
    let mut downloads = vec![DownloadItem {
        url: version.downloads.client.url.clone(),
        dest: client_jar.clone(),
        sha1: version.downloads.client.sha1.clone(),
    }];

    let libs = resolver::resolve_libraries(&libraries_dir, &version);
    downloads.extend(libs.downloads);
    let mut classpath = libs.classpath;
    let natives = libs.natives;

    let assets = resolver::resolve_assets(client, &assets_dir, &version)
        .await
        .map_err(|e| map_meta(e, ErrorCode::AssetIndexUnreachable, ErrorCode::AssetIndexInvalid))?;
    downloads.extend(assets.downloads);

    // Fabric loader + (for Celaris) Fabric API.
    let mut main_class = version.main_class.clone();
    let mut fabric_libs = Vec::new();
    let mut fabric_api_jar = None;
    if config.use_fabric {
        let loader = resolver::fabric_loader_version(client, &config.mc_version)
            .await
            .map_err(|e| map_meta(e, ErrorCode::FabricMetaUnreachable, ErrorCode::FabricMetaInvalid))?;
        reporter.log(&format!("Fabric loader {loader}"));
        let profile = resolver::fabric_profile(client, &config.mc_version, &loader)
            .await
            .map_err(|e| map_meta(e, ErrorCode::FabricMetaUnreachable, ErrorCode::FabricMetaInvalid))?;
        let (fabric_downloads, fabric_cp) = resolver::fabric_libraries(&libraries_dir, &profile);
        downloads.extend(fabric_downloads);
        fabric_libs = fabric_cp.clone();
        // Fabric libraries must precede the vanilla ones on the classpath.
        let mut merged = fabric_cp;
        merged.append(&mut classpath);
        classpath = merged;
        main_class = profile.main_class;

        if !config.mods.is_empty() {
            let (url, filename) = resolver::fabric_api(client, &config.mc_version)
                .await
                .map_err(|e| map_meta(e, ErrorCode::FabricMetaUnreachable, ErrorCode::FabricMetaInvalid))?;
            let dest = config.game_dir.join("mods").join(filename);
            downloads.push(DownloadItem {
                url,
                dest: dest.clone(),
                sha1: None,
            });
            fabric_api_jar = Some(dest);
        }
    }

    // The vanilla client jar goes at the end of the classpath.
    classpath.push(client_jar);

    Ok(LaunchPlan {
        main_class,
        classpath,
        natives,
        natives_dir,
        assets_dir,
        libraries_dir,
        assets_index_id: assets.index_id,
        downloads,
        fabric_libs,
        fabric_api_jar,
        version,
    })
}

fn map_meta(e: DownloadError, unreachable: ErrorCode, invalid: ErrorCode) -> LaunchError {
    match e {
        DownloadError::Http(m) | DownloadError::Io(m) => {
            LaunchError::new(Stage::Resolve, unreachable, m)
        }
        DownloadError::Parse(m) => LaunchError::new(Stage::Resolve, invalid, m),
        DownloadError::Sha1Mismatch { .. } => LaunchError::new(Stage::Resolve, invalid, e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Stage 2: download
// ---------------------------------------------------------------------------

async fn download_stage(
    plan: &LaunchPlan,
    client: &reqwest::Client,
    reporter: &dyn Reporter,
) -> StageResult<()> {
    announce(reporter, "download", "Lade Dateien…");
    download::download_many(client, plan.downloads.clone(), reporter, "download")
        .await
        .map_err(|e| match e {
            DownloadError::Sha1Mismatch { .. } => {
                LaunchError::new(Stage::Download, ErrorCode::Sha1Mismatch, e.to_string())
            }
            other => LaunchError::new(Stage::Download, ErrorCode::DownloadFailed, other.to_string()),
        })
}

// ---------------------------------------------------------------------------
// Stage 3: inject
// ---------------------------------------------------------------------------

fn inject_stage(
    config: &CelarisLaunchConfig,
    plan: &LaunchPlan,
    reporter: &dyn Reporter,
) -> StageResult<()> {
    announce(reporter, "inject", "Natives & Mods…");

    std::fs::create_dir_all(&plan.natives_dir)
        .map_err(|e| LaunchError::new(Stage::Inject, ErrorCode::NativesExtractFailed, e.to_string()))?;
    for jar in &plan.natives {
        extract_natives(jar, &plan.natives_dir)?;
    }

    let mods_dir = config.game_dir.join("mods");
    if !config.mods.is_empty() {
        std::fs::create_dir_all(&mods_dir)
            .map_err(|e| LaunchError::new(Stage::Inject, ErrorCode::ModMissing, e.to_string()))?;
        // Remove stale Celaris client jars from previous versions so only the
        // current one is loaded — otherwise Fabric loads several at once and an
        // outdated jar (with old mixins) crashes the game.
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("Celaris-") && name.ends_with(".jar") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        for jar in &config.mods {
            let file_name = jar.file_name().ok_or_else(|| {
                LaunchError::new(Stage::Inject, ErrorCode::ModMissing, format!("invalid mod path: {}", jar.display()))
            })?;
            std::fs::copy(jar, mods_dir.join(file_name)).map_err(|e| {
                LaunchError::new(Stage::Inject, ErrorCode::ModMissing, e.to_string())
            })?;
        }
        reporter.log(&format!("{} Mod(s) injiziert", config.mods.len()));
    }

    validate_injection(
        &mods_dir,
        &config.mods,
        plan.fabric_api_jar.as_deref(),
        &plan.fabric_libs,
    )
}

/// Asserts the post-inject filesystem state: every resolved mod, the Fabric API
/// and the Fabric loader jars are all present in `mods/` / on the classpath.
/// Pure (filesystem-only) so it is unit-testable.
pub(crate) fn validate_injection(
    mods_dir: &Path,
    expected_mods: &[PathBuf],
    fabric_api: Option<&Path>,
    fabric_libs: &[PathBuf],
) -> StageResult<()> {
    for jar in expected_mods {
        let present = jar
            .file_name()
            .map(|name| mods_dir.join(name).exists())
            .unwrap_or(false);
        if !present {
            return Err(LaunchError::new(
                Stage::Inject,
                ErrorCode::ModMissing,
                format!("mod jar missing from mods/: {}", jar.display()),
            ));
        }
    }
    if let Some(api) = fabric_api {
        if !api.exists() {
            return Err(LaunchError::new(
                Stage::Inject,
                ErrorCode::FabricApiMissing,
                format!("Fabric API jar missing: {}", api.display()),
            ));
        }
    }
    for lib in fabric_libs {
        if !lib.exists() {
            return Err(LaunchError::new(
                Stage::Inject,
                ErrorCode::FabricLoaderMissing,
                format!("Fabric loader jar missing: {}", lib.display()),
            ));
        }
    }
    Ok(())
}

fn extract_natives(jar: &Path, natives_dir: &Path) -> StageResult<()> {
    let err = |m: String| LaunchError::new(Stage::Inject, ErrorCode::NativesExtractFailed, m);
    let file = std::fs::File::open(jar).map_err(|e| err(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| err(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| err(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name.starts_with("META-INF") {
            continue;
        }
        if !(name.ends_with(".so") || name.ends_with(".dll") || name.ends_with(".dylib")) {
            continue;
        }
        let file_name = match Path::new(&name).file_name() {
            Some(n) => n,
            None => continue,
        };
        let out = natives_dir.join(file_name);
        let mut writer = std::fs::File::create(&out).map_err(|e| err(e.to_string()))?;
        std::io::copy(&mut entry, &mut writer).map_err(|e| err(e.to_string()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Stage 4: launch
// ---------------------------------------------------------------------------

fn launch_stage(
    config: &CelarisLaunchConfig,
    plan: &LaunchPlan,
    reporter: Arc<dyn Reporter>,
) -> StageResult<()> {
    announce(reporter.as_ref(), "launch", "Starte Minecraft…");

    let args = build_command(config, plan);
    let (program, rest) = args
        .split_first()
        .ok_or_else(|| LaunchError::new(Stage::Launch, ErrorCode::SpawnFailed, "empty command"))?;

    let mut child = spawn_validated(program, rest, &config.game_dir, &config.env)?;

    if let Some(stdout) = child.stdout.take() {
        stream(reporter.clone(), stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        stream(reporter.clone(), stderr);
    }
    std::thread::spawn(move || {
        let msg = match child.wait() {
            Ok(status) => format!("Minecraft beendet ({status})"),
            Err(e) => format!("Prozessfehler: {e}"),
        };
        reporter.log(&msg);
    });
    Ok(())
}

/// Spawns a process and validates it actually started: spawn must succeed and the
/// process must not exit with a failure within [`STARTUP_GRACE`]. Pure w.r.t. the
/// pipeline (no network) so it is unit-testable with trivial system binaries.
pub(crate) fn spawn_validated(
    program: &str,
    args: &[String],
    game_dir: &Path,
    env: &[(String, String)],
) -> StageResult<Child> {
    std::fs::create_dir_all(game_dir)
        .map_err(|e| LaunchError::new(Stage::Launch, ErrorCode::GameDirError, e.to_string()))?;

    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.clone(), v.clone())))
        .current_dir(game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| LaunchError::new(Stage::Launch, ErrorCode::SpawnFailed, e.to_string()))?;

    std::thread::sleep(STARTUP_GRACE);
    if let Ok(Some(status)) = child.try_wait() {
        if !status.success() {
            // The process died immediately — capture whatever it printed so the
            // real cause (Java error, missing class, incompatible version, …) is
            // shown instead of just an exit code.
            use std::io::Read;
            let mut out = String::new();
            if let Some(mut so) = child.stdout.take() {
                let _ = so.read_to_string(&mut out);
            }
            if let Some(mut se) = child.stderr.take() {
                let _ = se.read_to_string(&mut out);
            }
            let out = out.trim();
            let tail = if out.len() > 2000 { &out[out.len() - 2000..] } else { out };
            let detail = if tail.is_empty() {
                format!("process exited during startup: {status} (keine Ausgabe — evtl. falsche Java-Version)")
            } else {
                format!("process exited during startup: {status}\n{tail}")
            };
            return Err(LaunchError::new(Stage::Launch, ErrorCode::ProcessExitedEarly, detail));
        }
    }
    Ok(child)
}

fn build_command(config: &CelarisLaunchConfig, plan: &LaunchPlan) -> Vec<String> {
    // Classpath entries are separated by ';' on Windows and ':' elsewhere. Using
    // ':' on Windows mangled the whole classpath (drive letters contain ':'), so
    // the JVM found neither Main nor KnotClient → "ClassNotFoundException".
    let cp_sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    let cp = plan
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(cp_sep);

    let replacements: Vec<(&str, String)> = vec![
        ("${auth_player_name}", config.session.username.clone()),
        ("${version_name}", config.mc_version.clone()),
        ("${game_directory}", config.game_dir.to_string_lossy().to_string()),
        ("${assets_root}", plan.assets_dir.to_string_lossy().to_string()),
        ("${assets_index_name}", plan.assets_index_id.clone()),
        ("${auth_uuid}", config.session.uuid.clone()),
        ("${auth_access_token}", config.session.access_token.clone()),
        ("${clientid}", String::new()),
        ("${auth_xuid}", String::new()),
        ("${user_type}", config.session.user_type.clone()),
        ("${version_type}", "release".to_string()),
        ("${natives_directory}", plan.natives_dir.to_string_lossy().to_string()),
        ("${launcher_name}", "celaris-launcher".to_string()),
        ("${launcher_version}", "0.1".to_string()),
        ("${classpath}", cp.clone()),
        ("${classpath_separator}", ":".to_string()),
        ("${library_directory}", plan.libraries_dir.to_string_lossy().to_string()),
    ];

    let mut args: Vec<String> =
        vec![config.java_path.clone(), format!("-Xmx{}M", config.max_ram_mb)];
    args.extend(config.extra_jvm_args.iter().cloned());

    match &plan.version.arguments {
        Some(arguments) => {
            for token in resolver::collect_args(&arguments.jvm) {
                args.push(template(&token, &replacements));
            }
            args.push(plan.main_class.clone());
            for token in resolver::collect_args(&arguments.game) {
                args.push(template(&token, &replacements));
            }
        }
        None => {
            args.push(format!("-Djava.library.path={}", plan.natives_dir.to_string_lossy()));
            args.push("-cp".to_string());
            args.push(cp);
            args.push(plan.main_class.clone());
        }
    }
    // Direct-join: tell Minecraft to connect straight to a server on launch.
    if let Some(server) = config.quick_play_multiplayer.as_ref() {
        if !server.trim().is_empty() {
            args.push("--quickPlayMultiplayer".to_string());
            args.push(server.trim().to_string());
        }
    }
    args
}

fn template(token: &str, replacements: &[(&str, String)]) -> String {
    let mut out = token.to_string();
    for (key, value) in replacements {
        if out.contains(key) {
            out = out.replace(key, value);
        }
    }
    out
}

fn announce(reporter: &dyn Reporter, stage: &str, message: &str) {
    reporter.progress(Progress {
        stage: stage.to_string(),
        message: message.to_string(),
        current: 0,
        total: 0,
    });
    reporter.log(message);
}

fn stream<R: std::io::Read + Send + 'static>(reporter: Arc<dyn Reporter>, reader: R) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            reporter.log(&line);
        }
    });
}
