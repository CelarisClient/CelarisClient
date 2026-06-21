//! Resolves remote metadata into concrete download lists and launch arguments.
//!
//! The pure parsing/selection helpers ([`select_version`], [`parse_version_json`])
//! are separated from the network calls so they can be unit-tested deterministically
//! with fixtures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::download::{self, DownloadError, DownloadItem};

pub const VERSION_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_BASE: &str = "https://resources.download.minecraft.net";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const MODRINTH_API: &str = "https://api.modrinth.com/v2";

// ---------------------------------------------------------------------------
// Mojang version manifest + version json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    url: String,
    /// SHA1 of the per-version JSON — the integrity anchor for the resolve stage.
    sha1: String,
}

/// A located version entry: where to fetch the version JSON and its expected hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

/// Why [`select_version`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// Manifest JSON could not be parsed.
    Invalid(String),
    /// Manifest parsed but did not contain the requested version.
    NotFound(String),
}

/// Parses the version manifest and locates a version — pure and testable.
pub fn select_version(manifest_text: &str, mc_version: &str) -> Result<VersionRef, SelectError> {
    let manifest: VersionManifest =
        serde_json::from_str(manifest_text).map_err(|e| SelectError::Invalid(e.to_string()))?;
    manifest
        .versions
        .into_iter()
        .find(|v| v.id == mc_version)
        .map(|v| VersionRef {
            id: v.id,
            url: v.url,
            sha1: v.sha1,
        })
        .ok_or_else(|| SelectError::NotFound(mc_version.to_string()))
}

/// Parses a per-version client JSON — pure and testable.
pub fn parse_version_json(text: &str) -> Result<VersionJson, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct VersionJson {
    #[serde(rename = "mainClass")]
    pub main_class: String,
    pub assets: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
}

#[derive(Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Deserialize, Clone)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgValue>,
    #[serde(default)]
    pub jvm: Vec<ArgValue>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgVal },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ArgVal {
    One(String),
    Many(Vec<String>),
}

// ---------------------------------------------------------------------------
// Platform helpers / rules
// ---------------------------------------------------------------------------

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        "windows" => "windows",
        _ => "linux",
    }
}

fn os_matches(os: &OsRule) -> bool {
    let name_ok = os.name.as_deref().map(|n| n == os_name()).unwrap_or(true);
    let arch_ok = os
        .arch
        .as_deref()
        .map(|a| a == std::env::consts::ARCH)
        .unwrap_or(true);
    name_ok && arch_ok
}

fn rule_matches(rule: &Rule) -> bool {
    if rule.features.is_some() {
        return false; // no features enabled
    }
    rule.os.as_ref().map(os_matches).unwrap_or(true)
}

fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// `group:artifact:version[:classifier]` → relative maven path.
fn coord_to_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return name.to_string();
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar")
}

fn is_native_coord(name: &str) -> bool {
    name.split(':')
        .nth(3)
        .map(|c| c.starts_with("natives"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Library resolution
// ---------------------------------------------------------------------------

pub struct ResolvedLibraries {
    pub downloads: Vec<DownloadItem>,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<PathBuf>,
}

pub fn resolve_libraries(libraries_dir: &Path, version: &VersionJson) -> ResolvedLibraries {
    let mut downloads = Vec::new();
    let mut classpath = Vec::new();
    let mut natives = Vec::new();

    for lib in &version.libraries {
        if let Some(rules) = &lib.rules {
            if !rules_allow(rules) {
                continue;
            }
        }

        if let (Some(natives_map), Some(dl)) = (&lib.natives, &lib.downloads) {
            if let Some(classifier) = natives_map.get(os_name()) {
                let classifier = classifier.replace("${arch}", "64");
                if let Some(art) = dl.classifiers.as_ref().and_then(|c| c.get(&classifier)) {
                    let dest = artifact_dest(libraries_dir, &lib.name, art);
                    downloads.push(item(art, &dest));
                    natives.push(dest);
                }
            }
        }

        if let Some(dl) = &lib.downloads {
            if let Some(art) = &dl.artifact {
                let dest = artifact_dest(libraries_dir, &lib.name, art);
                downloads.push(item(art, &dest));
                if is_native_coord(&lib.name) {
                    natives.push(dest);
                } else {
                    classpath.push(dest);
                }
            }
        } else if let Some(base) = &lib.url {
            let rel = coord_to_path(&lib.name);
            let dest = libraries_dir.join(&rel);
            downloads.push(DownloadItem {
                url: format!("{}/{}", base.trim_end_matches('/'), rel),
                dest: dest.clone(),
                sha1: None,
            });
            classpath.push(dest);
        }
    }

    ResolvedLibraries {
        downloads,
        classpath,
        natives,
    }
}

fn artifact_dest(libraries_dir: &Path, name: &str, art: &Artifact) -> PathBuf {
    let rel = art.path.clone().unwrap_or_else(|| coord_to_path(name));
    libraries_dir.join(rel)
}

fn item(art: &Artifact, dest: &Path) -> DownloadItem {
    DownloadItem {
        url: art.url.clone(),
        dest: dest.to_path_buf(),
        sha1: art.sha1.clone(),
    }
}

// ---------------------------------------------------------------------------
// Assets
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AssetIndexFile {
    objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
}

pub struct ResolvedAssets {
    pub index_id: String,
    pub downloads: Vec<DownloadItem>,
}

/// Downloads + verifies the asset index, then enumerates object downloads.
pub async fn resolve_assets(
    client: &reqwest::Client,
    assets_dir: &Path,
    version: &VersionJson,
) -> Result<ResolvedAssets, DownloadError> {
    let index_ref = &version.asset_index;
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", index_ref.id));
    download::download_file(client, &index_ref.url, &index_path, Some(&index_ref.sha1)).await?;

    let index: AssetIndexFile = download::get_json(client, &index_ref.url).await?;

    let objects_dir = assets_dir.join("objects");
    let downloads = index
        .objects
        .into_values()
        .map(|obj| {
            let prefix = &obj.hash[0..2];
            DownloadItem {
                url: format!("{RESOURCES_BASE}/{prefix}/{}", obj.hash),
                dest: objects_dir.join(prefix).join(&obj.hash),
                sha1: Some(obj.hash.clone()),
            }
        })
        .collect();

    Ok(ResolvedAssets {
        index_id: version.assets.clone(),
        downloads,
    })
}

// ---------------------------------------------------------------------------
// Fabric
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderInfo,
}

#[derive(Deserialize)]
struct FabricLoaderInfo {
    version: String,
}

#[derive(Deserialize)]
pub struct FabricProfile {
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub libraries: Vec<FabricLibrary>,
}

#[derive(Deserialize)]
pub struct FabricLibrary {
    pub name: String,
    pub url: String,
}

pub async fn fabric_loader_version(
    client: &reqwest::Client,
    mc_version: &str,
) -> Result<String, DownloadError> {
    let url = format!("{FABRIC_META}/versions/loader/{mc_version}");
    let entries: Vec<FabricLoaderEntry> = download::get_json(client, &url).await?;
    entries
        .into_iter()
        .next()
        .map(|e| e.loader.version)
        .ok_or_else(|| DownloadError::Parse(format!("no Fabric loader for {mc_version}")))
}

pub async fn fabric_profile(
    client: &reqwest::Client,
    mc_version: &str,
    loader: &str,
) -> Result<FabricProfile, DownloadError> {
    let url = format!("{FABRIC_META}/versions/loader/{mc_version}/{loader}/profile/json");
    download::get_json(client, &url).await
}

pub fn fabric_libraries(
    libraries_dir: &Path,
    profile: &FabricProfile,
) -> (Vec<DownloadItem>, Vec<PathBuf>) {
    let mut downloads = Vec::new();
    let mut classpath = Vec::new();
    for lib in &profile.libraries {
        let rel = coord_to_path(&lib.name);
        let dest = libraries_dir.join(&rel);
        downloads.push(DownloadItem {
            url: format!("{}/{}", lib.url.trim_end_matches('/'), rel),
            dest: dest.clone(),
            sha1: None,
        });
        classpath.push(dest);
    }
    (downloads, classpath)
}

// ---------------------------------------------------------------------------
// Fabric API (mod jar from Modrinth) — required by the Celaris client mod
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ModrinthVersion {
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
}

pub async fn fabric_api(
    client: &reqwest::Client,
    mc_version: &str,
) -> Result<(String, String), DownloadError> {
    let url = format!("{MODRINTH_API}/project/fabric-api/version");
    let versions: Vec<ModrinthVersion> = download::get_json(client, &url).await?;
    let matched = versions
        .into_iter()
        .find(|v| {
            v.loaders.iter().any(|l| l == "fabric")
                && v.game_versions.iter().any(|g| g == mc_version)
        })
        .ok_or_else(|| DownloadError::Parse(format!("no Fabric API for {mc_version}")))?;

    let file = matched
        .files
        .into_iter()
        .find(|f| f.primary)
        .ok_or_else(|| DownloadError::Parse("Fabric API version has no primary file".into()))?;
    Ok((file.url, file.filename))
}

// ---------------------------------------------------------------------------
// Argument collection (rule-filtered, still untemplated)
// ---------------------------------------------------------------------------

pub fn collect_args(args: &[ArgValue]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            ArgValue::Plain(s) => out.push(s.clone()),
            ArgValue::Conditional { rules, value } => {
                if rules_allow(rules) {
                    match value {
                        ArgVal::One(s) => out.push(s.clone()),
                        ArgVal::Many(v) => out.extend(v.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}
