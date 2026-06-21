//! Mod marketplace (Modrinth). Separate from the frozen launch engine: it only
//! searches Modrinth and drops mod jars into the shared `usermods` pool that the
//! launch pipeline already scans. Reuses `launcher::download` for HTTP.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::launcher::{download, mods};

const MODRINTH: &str = "https://api.modrinth.com/v2";
const VERSION_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

// ---------------------------------------------------------------------------
// Minecraft version list (releases + snapshots + April-Fools, 1.20 era onward)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct McVersion {
    pub id: String,
    pub kind: String,
    pub release_time: String,
}

#[derive(Deserialize)]
struct ManifestList {
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "releaseTime")]
    release_time: String,
}

#[tauri::command]
pub async fn list_versions() -> Result<Vec<McVersion>, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let manifest: ManifestList = client
        .get(VERSION_MANIFEST)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    // 1.20 released 2023-06-07; ISO timestamps compare lexically. Manifest is
    // already newest-first, which is the order we want.
    const CUTOFF: &str = "2023-06-01";
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.release_time.as_str() >= CUTOFF)
        .map(|v| McVersion {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect())
}

/// Per-profile mod folder: `minecraft/usermods/<profile-slug>`. Each profile has
/// its own mod list; the launch injects exactly this profile's folder.
fn usermods_dir(app: &AppHandle, profile: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("minecraft")
        .join("usermods")
        .join(crate::instance_slug(profile));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ModHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    icon_url: Option<String>,
}

#[tauri::command]
pub async fn search_mods(query: String, mc_version: String, offset: Option<u32>, sort: Option<String>) -> Result<Vec<ModHit>, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let facets =
        format!(r#"[["project_type:mod"],["categories:fabric"],["versions:{mc_version}"]]"#);
    let off = offset.unwrap_or(0).to_string();
    let index = sort.filter(|s| !s.is_empty()).unwrap_or_else(|| "relevance".into());

    let resp: SearchResponse = client
        .get(format!("{MODRINTH}/search"))
        .query(&[
            ("query", query.as_str()),
            ("limit", "30"),
            ("offset", off.as_str()),
            ("index", index.as_str()),
            ("facets", facets.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    Ok(resp
        .hits
        .into_iter()
        .map(|h| ModHit {
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            author: h.author,
            downloads: h.downloads,
            icon_url: h.icon_url,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Version {
    files: Vec<VersionFile>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    date_published: String,
}

#[derive(Deserialize, Clone)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
}

/// Picks the first downloadable file of a version: prefer the primary, then the
/// first `.jar`/`.zip`, then any file. Fixes content whose version has no file
/// flagged "primary".
fn pick_file(v: &Version) -> Option<VersionFile> {
    v.files
        .iter()
        .find(|f| f.primary)
        .or_else(|| {
            v.files
                .iter()
                .find(|f| f.filename.ends_with(".jar") || f.filename.ends_with(".zip"))
        })
        .or_else(|| v.files.first())
        .cloned()
}

#[tauri::command]
pub async fn install_mod(
    app: AppHandle,
    project_id: String,
    mc_version: String,
    profile: String,
) -> Result<String, String> {
    let client = download::client().map_err(|e| e.to_string())?;

    // Fetch ALL versions (no server-side filter) and rank locally. The previous
    // strict loaders+game_versions query returned empty for many mods (e.g. ones
    // tagged only for a nearby MC patch, or whose loader list omits "fabric"),
    // which is why "some mods couldn't be downloaded". Ranking guarantees we pick
    // the best available file instead of failing.
    let versions: Vec<Version> = client
        .get(format!("{MODRINTH}/project/{project_id}/version"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if versions.is_empty() {
        return Err("Diese Mod hat keine veröffentlichten Versionen".to_string());
    }
    // Score: exact MC version match (+2) and fabric loader (+1) are best, then
    // fall back to the newest version that has any downloadable file.
    let best = versions
        .iter()
        .filter(|v| pick_file(v).is_some())
        .max_by_key(|v| {
            let ver = if v.game_versions.iter().any(|g| g == &mc_version) { 2 } else { 0 };
            let loader = if v.loaders.iter().any(|l| l == "fabric") { 1 } else { 0 };
            (ver + loader, v.date_published.clone())
        })
        .ok_or("Keine herunterladbare Datei für diese Mod gefunden")?;
    let file = pick_file(best).unwrap();

    let dest = usermods_dir(&app, &profile)?.join(&file.filename);
    download::download_file(&client, &file.url, &dest, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(file.filename)
}

// ---------------------------------------------------------------------------
// Installed mods (manage)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct InstalledMod {
    pub id: String,
    pub filename: String,
}

#[tauri::command]
pub fn list_installed_mods(app: AppHandle, profile: String) -> Result<Vec<InstalledMod>, String> {
    let dir = usermods_dir(&app, &profile)?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "jar").unwrap_or(false) {
                let filename = path.file_name().unwrap().to_string_lossy().to_string();
                let id = mods::read_fabric_mod_id(&path).unwrap_or_else(|| filename.clone());
                out.push(InstalledMod { id, filename });
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

#[tauri::command]
pub fn remove_mod(app: AppHandle, filename: String, profile: String) -> Result<(), String> {
    // Keep it inside the usermods dir (no traversal).
    let dir = usermods_dir(&app, &profile)?;
    let path = dir.join(&filename);
    if path.parent() != Some(dir.as_path()) {
        return Err("ungültiger Pfad".to_string());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

// ===========================================================================
// ResourcePacks + Shaders marketplaces (per-profile, Modrinth)
// ===========================================================================

/// A content folder inside a profile's instance game dir, e.g. `resourcepacks`
/// or `shaderpacks` (this is where Minecraft actually loads them from).
fn instance_content_dir(app: &AppHandle, profile: &str, sub: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("minecraft")
        .join("instances")
        .join(crate::instance_slug(profile))
        .join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Generic Modrinth search for a given project type ("resourcepack" | "shader").
async fn search_content(query: String, mc_version: String, project_type: &str, sort: Option<String>) -> Result<Vec<ModHit>, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let facets = format!(r#"[["project_type:{project_type}"],["versions:{mc_version}"]]"#);
    let index = sort.filter(|s| !s.is_empty()).unwrap_or_else(|| "relevance".into());
    let resp: SearchResponse = client
        .get(format!("{MODRINTH}/search"))
        .query(&[
            ("query", query.as_str()),
            ("limit", "30"),
            ("index", index.as_str()),
            ("facets", facets.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp
        .hits
        .into_iter()
        .map(|h| ModHit {
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            author: h.author,
            downloads: h.downloads,
            icon_url: h.icon_url,
        })
        .collect())
}

/// Downloads a project's newest matching file into a profile content folder.
/// `loaders` is an optional Modrinth loader filter (shaders use iris/optifine).
async fn install_content(
    app: &AppHandle,
    project_id: &str,
    mc_version: &str,
    profile: &str,
    sub: &str,
    loaders: Option<&str>,
) -> Result<String, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let mut q: Vec<(&str, String)> = vec![("game_versions", format!("[\"{mc_version}\"]"))];
    if let Some(l) = loaders {
        q.push(("loaders", l.to_string()));
    }
    let versions: Vec<Version> = client
        .get(format!("{MODRINTH}/project/{project_id}/version"))
        .query(&q)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let best = versions
        .iter()
        .filter(|v| pick_file(v).is_some())
        .max_by_key(|v| {
            let ver = if v.game_versions.iter().any(|g| g == mc_version) { 1 } else { 0 };
            (ver, v.date_published.clone())
        })
        .ok_or_else(|| format!("Keine mit {mc_version} kompatible Version gefunden"))?;
    let file = pick_file(best).ok_or("Version hat keine Hauptdatei")?;
    let dest = instance_content_dir(app, profile, sub)?.join(&file.filename);
    download::download_file(&client, &file.url, &dest, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(file.filename)
}

fn list_content(app: &AppHandle, profile: &str, sub: &str, exts: &[&str]) -> Result<Vec<InstalledMod>, String> {
    let dir = instance_content_dir(app, profile, sub)?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ok = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| exts.contains(&e))
                .unwrap_or(false);
            if ok {
                let filename = path.file_name().unwrap().to_string_lossy().to_string();
                out.push(InstalledMod { id: filename.clone(), filename });
            }
        }
    }
    out.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    Ok(out)
}

fn remove_content(app: &AppHandle, profile: &str, sub: &str, filename: &str) -> Result<(), String> {
    let dir = instance_content_dir(app, profile, sub)?;
    let path = dir.join(filename);
    if path.parent() != Some(dir.as_path()) {
        return Err("ungültiger Pfad".to_string());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

// --- ResourcePacks ---

#[tauri::command]
pub async fn search_resourcepacks(query: String, mc_version: String, sort: Option<String>) -> Result<Vec<ModHit>, String> {
    search_content(query, mc_version, "resourcepack", sort).await
}

#[tauri::command]
pub async fn install_resourcepack(app: AppHandle, project_id: String, mc_version: String, profile: String) -> Result<String, String> {
    install_content(&app, &project_id, &mc_version, &profile, "resourcepacks", None).await
}

#[tauri::command]
pub fn list_resourcepacks(app: AppHandle, profile: String) -> Result<Vec<InstalledMod>, String> {
    list_content(&app, &profile, "resourcepacks", &["zip"])
}

#[tauri::command]
pub fn remove_resourcepack(app: AppHandle, filename: String, profile: String) -> Result<(), String> {
    remove_content(&app, &profile, "resourcepacks", &filename)
}

// --- Shaders (only meaningful with Iris/OptiFine in the profile) ---

#[tauri::command]
pub async fn search_shaders(query: String, mc_version: String, sort: Option<String>) -> Result<Vec<ModHit>, String> {
    search_content(query, mc_version, "shader", sort).await
}

#[tauri::command]
pub async fn install_shader(app: AppHandle, project_id: String, mc_version: String, profile: String) -> Result<String, String> {
    install_content(&app, &project_id, &mc_version, &profile, "shaderpacks", Some("[\"iris\",\"optifine\"]")).await
}

#[tauri::command]
pub fn list_shaders(app: AppHandle, profile: String) -> Result<Vec<InstalledMod>, String> {
    list_content(&app, &profile, "shaderpacks", &["zip"])
}

#[tauri::command]
pub fn remove_shader(app: AppHandle, filename: String, profile: String) -> Result<(), String> {
    remove_content(&app, &profile, "shaderpacks", &filename)
}

/// True if the profile has a shader loader (Iris/Oculus/OptiFine) in its mod pool,
/// so the launcher only shows the shader marketplace when shaders can actually run.
#[tauri::command]
pub fn profile_has_shaders(app: AppHandle, profile: String) -> Result<bool, String> {
    let dir = usermods_dir(&app, &profile)?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".jar")
                && (name.contains("iris") || name.contains("oculus") || name.contains("optifine"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
