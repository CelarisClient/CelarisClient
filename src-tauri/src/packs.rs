//! Modpack import / export.
//!
//! Imports Modrinth `.mrpack` packs and the native `.celarispack` format, each as a
//! self-contained profile under `minecraft/instances/<name>`; exports a profile
//! (with its mod jars bundled) to `.celarispack`. Separate from the frozen engine.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::launcher::download;
use crate::{Profile, CONTENT_BASE};

fn minecraft_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("minecraft"))
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "pack".to_string() } else { s }
}

// ---------------------------------------------------------------------------
// Manifest shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MrIndex {
    name: String,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    files: Vec<MrFile>,
}

#[derive(Deserialize)]
struct MrFile {
    path: String,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    hashes: Option<MrHashes>,
    #[serde(default)]
    env: Option<MrEnv>,
}

#[derive(Deserialize)]
struct MrHashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Deserialize)]
struct MrEnv {
    #[serde(default)]
    client: Option<String>,
}

/// The native Celaris pack manifest (`celaris.json`).
#[derive(Serialize, Deserialize)]
struct CelarisPack {
    format: String,
    format_version: u32,
    name: String,
    minecraft_version: String,
    loader: String,
    use_celaris_client: bool,
    max_ram_mb: u32,
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn import_modpack(app: AppHandle, path: String) -> Result<Profile, String> {
    let p = PathBuf::from(&path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "mrpack" => import_mrpack(&app, &p).await,
        "celarispack" | "zip" => import_celarispack(&app, &p),
        other => Err(format!("Unbekanntes Format: .{other}")),
    }
}

async fn import_mrpack(app: &AppHandle, path: &Path) -> Result<Profile, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let index: MrIndex = read_json(&mut zip, "modrinth.index.json")?;
    let mc = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or("Pack nennt keine Minecraft-Version")?;
    let fabric = index.dependencies.contains_key("fabric-loader");

    let game_dir = minecraft_root(app)?
        .join("instances")
        .join(sanitize(&index.name));
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    let client = download::client().map_err(|e| e.to_string())?;
    for f in &index.files {
        if f.env.as_ref().and_then(|e| e.client.as_deref()) == Some("unsupported") {
            continue;
        }
        let url = f
            .downloads
            .first()
            .ok_or_else(|| format!("Keine Download-URL für {}", f.path))?;
        let dest = game_dir.join(&f.path);
        let sha1 = f.hashes.as_ref().and_then(|h| h.sha1.clone());
        download::download_file(&client, url, &dest, sha1.as_deref())
            .await
            .map_err(|e| e.to_string())?;
    }

    extract_prefixed(&mut zip, "overrides/", &game_dir)?;
    extract_prefixed(&mut zip, "client-overrides/", &game_dir)?;

    Ok(Profile {
        name: index.name,
        minecraft_version: mc,
        java_path: "java".into(),
        max_ram_mb: 4096,
        game_dir: game_dir.to_string_lossy().to_string(),
        use_celaris_client: false,
        use_fabric: fabric,
        jvm_args: String::new(),
        env_vars: String::new(),
    })
}

fn import_celarispack(app: &AppHandle, path: &Path) -> Result<Profile, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let manifest: CelarisPack = read_json(&mut zip, "celaris.json")
        .map_err(|_| "celaris.json fehlt (kein gültiges .celarispack)".to_string())?;

    let game_dir = minecraft_root(app)?
        .join("instances")
        .join(sanitize(&manifest.name));
    std::fs::create_dir_all(game_dir.join("mods")).map_err(|e| e.to_string())?;

    extract_prefixed(&mut zip, "mods/", &game_dir.join("mods"))?;
    extract_prefixed(&mut zip, "overrides/", &game_dir)?;

    Ok(Profile {
        name: manifest.name,
        minecraft_version: manifest.minecraft_version,
        java_path: "java".into(),
        max_ram_mb: manifest.max_ram_mb,
        game_dir: game_dir.to_string_lossy().to_string(),
        use_celaris_client: manifest.use_celaris_client,
        use_fabric: manifest.loader == "fabric",
        jvm_args: String::new(),
        env_vars: String::new(),
    })
}

// ---------------------------------------------------------------------------
// Export → .celarispack
// ---------------------------------------------------------------------------

/// What to include in an export. The launcher asks the user before exporting.
#[derive(Deserialize)]
pub struct ExportOptions {
    #[serde(default = "yes")]
    pub mods: bool,
    #[serde(default = "yes")]
    pub resourcepacks: bool,
    #[serde(default = "yes")]
    pub shaderpacks: bool,
    #[serde(default = "yes")]
    pub config: bool,
    #[serde(default = "yes")]
    pub options: bool,
}

fn yes() -> bool {
    true
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { mods: true, resourcepacks: true, shaderpacks: true, config: true, options: true }
    }
}

/// The directory Minecraft runs in for a profile (must match the launch logic).
fn profile_game_dir(app: &AppHandle, profile: &Profile) -> Result<PathBuf, String> {
    if profile.game_dir.trim().is_empty() {
        Ok(minecraft_root(app)?.join("instances").join(crate::instance_slug(&profile.name)))
    } else {
        Ok(PathBuf::from(&profile.game_dir))
    }
}

#[tauri::command]
pub fn export_celarispack(
    app: AppHandle,
    profile: Profile,
    dest: String,
    opts: Option<ExportOptions>,
) -> Result<(), String> {
    let opts = opts.unwrap_or_default();
    let game_dir = profile_game_dir(&app, &profile)?;

    let manifest = CelarisPack {
        format: "celarispack".into(),
        format_version: 1,
        name: profile.name.clone(),
        minecraft_version: profile.minecraft_version.clone(),
        loader: if profile.use_celaris_client || profile.use_fabric {
            "fabric".into()
        } else {
            "vanilla".into()
        },
        use_celaris_client: profile.use_celaris_client,
        max_ram_mb: profile.max_ram_mb,
    };

    let file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let zopts = zip::write::SimpleFileOptions::default();

    zip.start_file("celaris.json", zopts).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    // Mods → mods/<jar>
    if opts.mods {
        for jar in collect_mod_jars(&app, &profile)? {
            let fname = jar.file_name().and_then(|n| n.to_str()).unwrap_or("mod.jar").to_string();
            zip.start_file(format!("mods/{fname}"), zopts).map_err(|e| e.to_string())?;
            let bytes = std::fs::read(&jar).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }

    // Everything else goes under overrides/ so import restores it verbatim.
    if opts.resourcepacks {
        let d = game_dir.join("resourcepacks");
        add_dir_to_zip(&mut zip, zopts, &d, &d, "overrides/resourcepacks")?;
    }
    if opts.shaderpacks {
        let d = game_dir.join("shaderpacks");
        add_dir_to_zip(&mut zip, zopts, &d, &d, "overrides/shaderpacks")?;
    }
    if opts.config {
        let d = game_dir.join("config");
        add_dir_to_zip(&mut zip, zopts, &d, &d, "overrides/config")?;
    }
    if opts.options {
        let o = game_dir.join("options.txt");
        if o.exists() {
            zip.start_file("overrides/options.txt", zopts).map_err(|e| e.to_string())?;
            zip.write_all(&std::fs::read(&o).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Recursively adds every file under `cur` into the zip at `<zip_prefix>/<rel>`,
/// where `rel` is the path relative to `base`. No-op if the directory is missing.
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    zopts: zip::write::SimpleFileOptions,
    base: &Path,
    cur: &Path,
    zip_prefix: &str,
) -> Result<(), String> {
    if !cur.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(cur).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(zip, zopts, base, &path, zip_prefix)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(format!("{zip_prefix}/{rel}"), zopts).map_err(|e| e.to_string())?;
            zip.write_all(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_json<T: serde::de::DeserializeOwned>(
    zip: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> Result<T, String> {
    let mut entry = zip
        .by_name(name)
        .map_err(|_| format!("{name} fehlt im Pack"))?;
    let mut s = String::new();
    entry.read_to_string(&mut s).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// Extracts every entry under `prefix` into `dest_root`, preserving sub-paths.
fn extract_prefixed(
    zip: &mut zip::ZipArchive<std::fs::File>,
    prefix: &str,
    dest_root: &Path,
) -> Result<(), String> {
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let rel = match name.strip_prefix(prefix) {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };
        let out = dest_root.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut writer = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut writer).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn collect_mod_jars(app: &AppHandle, profile: &Profile) -> Result<Vec<PathBuf>, String> {
    let mut jars = Vec::new();
    let mut seen = HashSet::new();
    add_jars(&minecraft_root(app)?.join("usermods"), &mut jars, &mut seen);
    if !profile.game_dir.is_empty() {
        add_jars(&PathBuf::from(&profile.game_dir).join("mods"), &mut jars, &mut seen);
    }
    Ok(jars)
}

fn add_jars(dir: &Path, jars: &mut Vec<PathBuf>, seen: &mut HashSet<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "jar").unwrap_or(false) {
                let fname = path.file_name().unwrap().to_string_lossy().to_string();
                if seen.insert(fname) {
                    jars.push(path);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Global modpacks (admin-curated, available to everyone)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct GlobalPack {
    pub name: String,
    #[serde(default)]
    pub mc_version: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    /// Download URL of the `.mrpack` or `.celarispack`.
    pub url: String,
}

#[tauri::command]
pub async fn list_global_modpacks() -> Result<Vec<GlobalPack>, String> {
    let client = match download::client() {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };
    let resp = client
        .get(format!("{CONTENT_BASE}/modpacks.json"))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status());
    match resp {
        Ok(r) => Ok(r.json::<Vec<GlobalPack>>().await.unwrap_or_default()),
        Err(_) => Ok(Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// Server-curated modpacks (DB-backed, from /api/modpacks)
// ---------------------------------------------------------------------------

/// API base (content host minus the `/content` suffix).
fn api_base() -> String {
    CONTENT_BASE.trim_end_matches("/content").to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServerModpack {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mc_version: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub mods: Vec<ModRef>,
    #[serde(default)]
    pub server_address: String,
    /// If set, an uploaded .mrpack/.celarispack to download + import directly.
    #[serde(default)]
    pub file_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModRef {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
}

#[tauri::command]
pub async fn list_server_modpacks() -> Result<Vec<ServerModpack>, String> {
    let client = match download::client() {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };
    let resp = client
        .get(format!("{}/api/modpacks", api_base()))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status());
    match resp {
        Ok(r) => Ok(r.json::<Vec<ServerModpack>>().await.unwrap_or_default()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Installs a server modpack: creates a profile, installs each listed Modrinth
/// mod into it, and adds the preset's server (if any) to the server list.
#[tauri::command]
pub async fn install_server_modpack(app: AppHandle, slug: String) -> Result<Profile, String> {
    let pack = list_server_modpacks()
        .await?
        .into_iter()
        .find(|m| m.slug == slug)
        .ok_or("Modpack nicht gefunden")?;

    // If an uploaded pack file is attached, download + import it directly.
    if !pack.file_url.trim().is_empty() {
        return install_global_modpack(app, pack.file_url.clone()).await;
    }

    let mc_version = if pack.mc_version.trim().is_empty() {
        "1.21.11".to_string()
    } else {
        pack.mc_version.clone()
    };

    let profile = Profile {
        name: pack.name.clone(),
        minecraft_version: mc_version.clone(),
        java_path: String::new(),
        max_ram_mb: 4096,
        game_dir: String::new(),
        use_celaris_client: true,
        use_fabric: true,
        jvm_args: String::new(),
        env_vars: String::new(),
    };

    // Register the profile (replace one with the same name, if any).
    let mut profiles = crate::get_profiles(app.clone())?;
    profiles.retain(|p| p.name != profile.name);
    profiles.push(profile.clone());
    crate::save_profiles(app.clone(), profiles)?;

    // Install each mod from Modrinth into the new profile's pool.
    for m in &pack.mods {
        if m.project_id.trim().is_empty() {
            continue;
        }
        let _ = crate::store::install_mod(
            app.clone(),
            m.project_id.clone(),
            mc_version.clone(),
            profile.name.clone(),
            None,
            None,
        )
        .await;
    }

    // Add the preset's server so it shows up in-game.
    if !pack.server_address.trim().is_empty() {
        let mut servers = crate::servers::get_servers(app.clone()).unwrap_or_default();
        if !servers.iter().any(|s| s.address.eq_ignore_ascii_case(&pack.server_address)) {
            servers.push(crate::servers::ServerEntry {
                name: pack.name.clone(),
                address: pack.server_address.clone(),
                partner: true,
                description: if pack.description.is_empty() { None } else { Some(pack.description.clone()) },
                icon: None,
                banner: None,
            });
            let _ = crate::servers::save_servers(app.clone(), servers);
        }
    }

    Ok(profile)
}

/// Downloads a global modpack and imports it through the normal pack importer.
#[tauri::command]
pub async fn install_global_modpack(app: AppHandle, url: String) -> Result<Profile, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let ext = if url.to_lowercase().ends_with(".celarispack") {
        "celarispack"
    } else {
        "mrpack"
    };
    let tmp_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let tmp = tmp_dir.join(format!("globalpack.{ext}"));

    download::download_file(&client, &url, &tmp, None)
        .await
        .map_err(|e| e.to_string())?;
    import_modpack(app, tmp.to_string_lossy().to_string()).await
}
