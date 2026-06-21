//! Auto-update for the in-game client mod. On every launch the launcher checks
//! the backend's `<CONTENT_BASE>/celaris/version.json` and downloads the newest
//! mod jar if it differs from the locally cached one — so new in-game features
//! ship without any launcher update. Old cached jars are deleted so only the
//! current one is ever injected. Falls back to the locally built jar when the
//! backend is unreachable.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::launcher::download;
use crate::CONTENT_BASE;

#[derive(Deserialize)]
struct ClientVersion {
    version: String,
    /// Jar filename on the host (defaults to `Celaris-<version>.jar`).
    #[serde(default)]
    file: String,
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("minecraft")
        .join("celaris-client");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Bundles ViaFabricPlus so the Celaris client can join servers of almost any
/// Minecraft version (1.7 → latest). Downloaded once per MC version from Modrinth
/// into a separate compat cache. Best-effort: None if unavailable/offline.
pub async fn ensure_viafabricplus(app: &AppHandle, mc_version: &str) -> Option<PathBuf> {
    #[derive(Deserialize)]
    struct V {
        files: Vec<F>,
    }
    #[derive(Deserialize)]
    struct F {
        url: String,
        filename: String,
        #[serde(default)]
        primary: bool,
    }

    let client = download::client().ok()?;
    let versions: Vec<V> = client
        .get("https://api.modrinth.com/v2/project/viafabricplus/version")
        .query(&[
            ("loaders", "[\"fabric\"]"),
            ("game_versions", &format!("[\"{mc_version}\"]")),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    let file = versions.into_iter().next()?.files.into_iter().find(|f| f.primary)?;
    let dir = app
        .path()
        .app_data_dir()
        .ok()?
        .join("minecraft")
        .join("compat");
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(&file.filename);
    if !dest.exists() {
        download::download_file(&client, &file.url, &dest, None).await.ok()?;
    }
    Some(dest)
}

/// Performance/optimisation mods bundled with the Celaris client (Modrinth slugs).
/// Downloaded per MC version into a managed cache and injected automatically.
const PERFORMANCE_MODS: &[&str] = &[
    "sodium",
    "lithium",
    "ferrite-core",
    "modernfix",
    "immediatelyfast",
    "entityculling",
    "moreculling",
    "c2me-fabric",
    "noisium",
    "ebe",          // Enhanced Block Entities
    "memoryleakfix",
    "lazydfu",
];

#[derive(Deserialize)]
struct MrVersion {
    files: Vec<MrFile>,
}
#[derive(Deserialize, Clone)]
struct MrFile {
    url: String,
    #[serde(default)]
    primary: bool,
}

/// Downloads + caches the bundled performance mods for `mc_version` and returns
/// their jar paths. Each is best-effort: a mod with no build for this version is
/// simply skipped (e.g. LazyDFU on newer MC). Cached per version so it only
/// downloads once.
pub async fn ensure_performance_mods(app: &AppHandle, mc_version: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(client) = download::client() else {
        return out;
    };
    let Ok(base) = app.path().app_data_dir() else {
        return out;
    };
    let dir = base.join("minecraft").join("compat").join("perf").join(mc_version);
    if std::fs::create_dir_all(&dir).is_err() {
        return out;
    }

    for slug in PERFORMANCE_MODS {
        if let Some(p) = ensure_perf_mod(&client, &dir, slug, mc_version).await {
            out.push(p);
        }
    }
    out
}

async fn ensure_perf_mod(
    client: &reqwest::Client,
    dir: &std::path::Path,
    slug: &str,
    mc_version: &str,
) -> Option<PathBuf> {
    // Reuse an already-cached copy (named after the slug) if present.
    let cached = dir.join(format!("{slug}.jar"));
    if cached.exists() {
        return Some(cached);
    }
    let versions: Vec<MrVersion> = client
        .get(format!("https://api.modrinth.com/v2/project/{slug}/version"))
        .query(&[
            ("loaders", "[\"fabric\"]"),
            ("game_versions", &format!("[\"{mc_version}\"]")),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    // Newest matching version, primary file (else first).
    let file = versions.into_iter().find_map(|v| {
        v.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| v.files.first())
            .cloned()
    })?;
    download::download_file(client, &file.url, &cached, None).await.ok()?;
    Some(cached)
}

/// Returns the path to the up-to-date client jar, downloading it from the backend
/// if a newer version is published. `None` on any failure (caller falls back to
/// the bundled jar).
pub async fn ensure_latest(app: &AppHandle) -> Option<PathBuf> {
    let client = download::client().ok()?;
    let meta: ClientVersion = client
        .get(format!("{CONTENT_BASE}/celaris/version.json"))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    let dir = cache_dir(app).ok()?;
    let file = if meta.file.is_empty() {
        format!("Celaris-{}.jar", meta.version)
    } else {
        meta.file.clone()
    };
    let dest = dir.join(&file);
    let ver_file = dir.join("version.txt");
    let local = std::fs::read_to_string(&ver_file).unwrap_or_default();

    if local.trim() != meta.version || !dest.exists() {
        download::download_file(&client, &format!("{CONTENT_BASE}/celaris/{file}"), &dest, None)
            .await
            .ok()?;
        purge_old_jars(&dir, &dest);
        let _ = std::fs::write(&ver_file, meta.version.trim());
    }

    dest.exists().then_some(dest)
}

/// Deletes every cached jar except `keep`, so only the current mod jar is injected.
fn purge_old_jars(dir: &std::path::Path, keep: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p != keep && p.extension().map(|x| x == "jar").unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}
