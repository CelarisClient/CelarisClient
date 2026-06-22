#[cfg(feature = "admin")]
mod admin;
mod clientmod;
mod coins;
mod java;
mod launcher;
mod news;
mod packs;
mod servers;
mod social;
mod skins;
mod spotify;
mod store;

/// Base URL for admin-managed remote content (partner servers, global modpacks,
/// announcements). Served over HTTP as JSON — the launcher NEVER talks to a DB
/// directly. Replace with your own hosting once available.
pub(crate) const CONTENT_BASE: &str = "https://api.celarisclient.de/content";

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use launcher::{auth, mods, CelarisLaunchConfig, Progress, Reporter, Session};
use launcher::auth::AuthProvider;

/// ---------------- MODELS ----------------

#[derive(Debug, Clone, Serialize)]
pub struct JavaInstall {
    pub path: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub minecraft_version: String,
    pub java_path: String,
    pub max_ram_mb: u32,
    pub game_dir: String,
    #[serde(default, alias = "use_atlas_client")]
    pub use_celaris_client: bool,
    /// Use the Fabric loader without necessarily injecting the Celaris client
    /// (set by modpack imports). Celaris profiles imply Fabric regardless.
    #[serde(default)]
    pub use_fabric: bool,
    /// Extra JVM arguments, whitespace/newline separated.
    #[serde(default)]
    pub jvm_args: String,
    /// Extra environment variables, one `KEY=VALUE` per line.
    #[serde(default)]
    pub env_vars: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ProfileStore {
    profiles: Vec<Profile>,
}

/// A saved account the user can switch between (offline or Microsoft).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// "offline" or "microsoft".
    pub kind: String,
    pub username: String,
    pub uuid: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub user_type: String,
    /// OAuth refresh token for Microsoft accounts. Persisted so the launcher can
    /// silently mint a fresh Minecraft session at launch (stay-logged-in) instead
    /// of failing with a 401. Empty for offline accounts. Was previously dropped
    /// here, which is why online accounts lost their login after a restart.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AccountStore {
    accounts: Vec<Account>,
}

/// ---------------- PATH ----------------

fn profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("profiles.json"))
}

/// ---------------- JAVA ----------------

#[tauri::command]
fn detect_java() -> Vec<JavaInstall> {
    let mut candidates = vec![];

    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin/java"));
    }

    candidates.push(PathBuf::from("java"));

    if let Ok(jvms) = std::fs::read_dir("/usr/lib/jvm") {
        for j in jvms.flatten() {
            candidates.push(j.path().join("bin/java"));
        }
    }

    let mut out = vec![];
    let mut seen = vec![];

    for path in candidates {
        if let Some(version) = java_version(&path) {
            let key = path.to_string_lossy().to_string();
            if !seen.contains(&key) {
                seen.push(key.clone());
                out.push(JavaInstall { path: key, version });
            }
        }
    }

    out
}

fn java_version(java: &Path) -> Option<String> {
    let mut cmd = Command::new(java);
    cmd.arg("-version");
    // No console flash on Windows for the version probe.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let first = text.lines().next()?;

    first.split('"')
        .nth(1)
        .map(|s| s.to_string())
        .or_else(|| Some(first.to_string()))
}

/// ---------------- PROFILE IO ----------------

#[tauri::command]
fn get_profiles(app: AppHandle) -> Result<Vec<Profile>, String> {
    let path = profiles_path(&app)?;

    if !path.exists() {
        return Ok(vec![]);
    }

    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let store: ProfileStore = serde_json::from_str(&data).map_err(|e| e.to_string())?;

    Ok(store.profiles)
}

#[tauri::command]
fn save_profiles(app: AppHandle, profiles: Vec<Profile>) -> Result<(), String> {
    let path = profiles_path(&app)?;

    let json = serde_json::to_string_pretty(&ProfileStore { profiles })
        .map_err(|e| e.to_string())?;

    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Returns (creating if needed) the per-profile mods folder where users can drop
/// their own `.jar` mods — exactly what the launcher loads at launch. Lets the UI
/// show the path and open it in the file manager.
#[tauri::command]
fn mods_dir(app: AppHandle, profile: Profile, open: Option<bool>) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("minecraft")
        .join("usermods")
        .join(instance_slug(&profile.name));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.to_string_lossy().to_string();
    // Open in the file manager from Rust so we bypass the JS opener path ACL.
    if open.unwrap_or(false) {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_path(path.clone(), None::<&str>)
            .map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// ---------------- ACCOUNTS ----------------

fn accounts_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("accounts.json"))
}

#[tauri::command]
fn get_accounts(app: AppHandle) -> Result<Vec<Account>, String> {
    let path = accounts_path(&app)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let store: AccountStore = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(store.accounts)
}

#[tauri::command]
fn save_accounts(app: AppHandle, accounts: Vec<Account>) -> Result<(), String> {
    let path = accounts_path(&app)?;
    let json = serde_json::to_string_pretty(&AccountStore { accounts }).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Builds a deterministic offline session for a chosen username (independent of
/// the profile name) — used both for "add offline account" and at launch.
#[tauri::command]
fn offline_session(username: String) -> Session {
    Session::offline(&username)
}

/// ---------------- REPORTER ----------------

struct TauriReporter {
    app: AppHandle,
}

impl Reporter for TauriReporter {
    fn progress(&self, p: Progress) {
        let _ = self.app.emit("launch-progress", p);
    }

    fn log(&self, l: &str) {
        let _ = self.app.emit("launch-log", l.to_string());
    }
}

/// ---------------- CELARIS JAR ----------------

/// Picks the Celaris client jar for a given Minecraft version. Prefers a jar whose
/// filename contains the version (e.g. `Celaris-1.21.11.jar`); falls back to the
/// newest Celaris jar. Returns None when no Celaris build is available.
fn find_celaris_jar(mc_version: &str) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CELARIS_CLIENT_JAR") {
        let p = PathBuf::from(explicit);
        if p.exists() {
            return Some(p);
        }
    }

    let cwd = std::env::current_dir().ok();
    let mut candidates = vec![PathBuf::from(
        "/home/edoreki/IdeaProjects/Celaris/celaris-client/build/libs",
    )];
    if let Some(cwd) = cwd {
        candidates.push(cwd.join("../../celaris-client/build/libs"));
        candidates.push(cwd.join("../celaris-client/build/libs"));
    }

    let mut all: Vec<PathBuf> = Vec::new();
    for dir in candidates {
        collect_celaris_jars(&dir, &mut all);
    }

    // Prefer an exact version-matched build, else the newest.
    if let Some(matched) = all.iter().find(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().contains(mc_version))
            .unwrap_or(false)
    }) {
        return Some(matched.clone());
    }
    all.into_iter()
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

fn collect_celaris_jars(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_string());
            if let Some(name) = name {
                // Accept both the legacy "Celaris-*.jar" and the rebranded "Celaris-*.jar".
                if (name.starts_with("Celaris") || name.starts_with("Celaris"))
                    && name.ends_with(".jar")
                    && !name.contains("sources")
                {
                    out.push(p);
                }
            }
        }
    }
}

/// ---------------- CONFIG ----------------

/// Splits free-form JVM-argument text (whitespace/newline separated) into args.
fn parse_jvm_args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(|s| s.to_string()).collect()
}

/// Parses `KEY=VALUE` lines into environment-variable pairs. Blank lines and
/// lines starting with `#` are ignored; the value may itself contain `=`.
fn parse_env_vars(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            l.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Filesystem-safe slug for a profile name, used as its instance folder so
/// distinct profiles get distinct game dirs and can run simultaneously.
pub(crate) fn instance_slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "default".into() } else { slug }
}

/// Minimum Java major a given Minecraft version needs.
/// (1.20.5+/1.21+ → 21, 1.18–1.20.4 → 17, 1.17 → 16, older → 8.)
fn required_java_major(mc: &str) -> u32 {
    let nums: Vec<u32> = mc
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let major = nums.first().copied().unwrap_or(1);
    // Mojang's new year-based versioning (e.g. "26.1.2") replaced the old
    // "1.MINOR.PATCH" scheme after 1.21.x. These newer releases need Java 25.
    // Without this, "26.1.2" parsed as minor=1 and fell through to Java 8 → the
    // game crashed instantly with no log.
    if major != 1 {
        return 25;
    }
    let minor = nums.get(1).copied().unwrap_or(0);
    let patch = nums.get(2).copied().unwrap_or(0);
    if minor >= 21 {
        21
    } else if minor == 20 && patch >= 5 {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

/// Authoritative required Java major for a Minecraft version: reads
/// `javaVersion.majorVersion` from Mojang's version JSON. Falls back to the
/// heuristic offline. This is what makes any version (1.21.x, 26.x, …) pick the
/// correct JRE instead of guessing.
async fn required_java(mc_version: &str) -> u32 {
    let fallback = required_java_major(mc_version);
    let Ok(http) = launcher::download::client() else {
        return fallback;
    };
    let manifest: serde_json::Value = match http
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(_) => return fallback,
        },
        Err(_) => return fallback,
    };
    let url = manifest
        .get("versions")
        .and_then(|a| a.as_array())
        .and_then(|a| a.iter().find(|v| v.get("id").and_then(|i| i.as_str()) == Some(mc_version)))
        .and_then(|v| v.get("url"))
        .and_then(|u| u.as_str());
    let Some(url) = url else {
        return fallback;
    };
    match http.get(url).timeout(std::time::Duration::from_secs(6)).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(vj) => vj
                .get("javaVersion")
                .and_then(|j| j.get("majorVersion"))
                .and_then(|n| n.as_u64())
                .map(|n| n as u32)
                .unwrap_or(fallback),
            Err(_) => fallback,
        },
        Err(_) => fallback,
    }
}

/// Parses a Java major version out of a `-version` string ("21.0.11", "1.8.0_x").
fn java_major_of(version: &str) -> Option<u32> {
    let v = version.trim();
    let first: u32 = v.split('.').next()?.parse().ok()?;
    if first == 1 {
        v.split('.').nth(1)?.parse().ok()
    } else {
        Some(first)
    }
}

/// Picks the best installed JDK for the MC version. Honors an explicit user
/// `java_path`; otherwise prefers an exact major match (avoids too-new JDKs that
/// break LWJGL/VulkanMod natives), then the next-smallest major ≥ required.
fn resolve_java(mc_version: &str, configured: &str) -> String {
    if !configured.is_empty() && configured != "java" {
        return configured.to_string();
    }
    let need = required_java_major(mc_version);
    let mut fallback: Option<(u32, String)> = None;
    for inst in detect_java() {
        if let Some(maj) = java_major_of(&inst.version) {
            if maj == need {
                return inst.path;
            }
            if maj > need && fallback.as_ref().map(|(m, _)| maj < *m).unwrap_or(true) {
                fallback = Some((maj, inst.path));
            }
        }
    }
    fallback
        .map(|(_, p)| p)
        .unwrap_or_else(|| if configured.is_empty() { "java".into() } else { configured.to_string() })
}

fn profile_to_config(
    profile: &Profile,
    root: PathBuf,
    game: PathBuf,
    session: Session,
    mods: Vec<PathBuf>,
) -> CelarisLaunchConfig {
    let java_path = resolve_java(&profile.minecraft_version, &profile.java_path);

    // Pass profile + client flag to the mod so it can set the window title
    // ("<profile> as <account> with CelarisClient"). Each is a single argv entry,
    // so spaces in the name are fine (no shell splitting).
    let mut jvm = parse_jvm_args(&profile.jvm_args);
    jvm.push(format!("-Dcelaris.profile={}", profile.name));
    jvm.push(format!("-Dcelaris.client={}", profile.use_celaris_client));

    CelarisLaunchConfig {
        mc_version: profile.minecraft_version.clone(),
        java_path,
        max_ram_mb: profile.max_ram_mb,
        session,
        game_dir: game,
        root_dir: root,
        // Celaris needs Fabric; modpacks may want Fabric without the Celaris client.
        use_fabric: profile.use_celaris_client || profile.use_fabric,
        mods,
        extra_jvm_args: jvm,
        env: parse_env_vars(&profile.env_vars),
        quick_play_multiplayer: None,
    }
}

/// ---------------- LAUNCH ----------------

#[tauri::command]
async fn launch(
    app: AppHandle,
    profile: Profile,
    session: Option<Session>,
    server: Option<String>,
) -> Result<(), String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("minecraft");

    // Per-profile instance dir → distinct profiles can run at the same time.
    let game = if profile.game_dir.is_empty() {
        root.join("instances").join(instance_slug(&profile.name))
    } else {
        PathBuf::from(&profile.game_dir)
    };

    let username = if profile.name.is_empty() {
        "Player".into()
    } else {
        profile.name.clone()
    };

    let session = match session {
        Some(s) => s,
        None => auth::OfflineProvider::new(username.clone())
            .authenticate()
            .await
            .unwrap_or_else(|_| Session::offline(&username)),
    };

    // Always refresh the in-game server list (partners + user) before launch so
    // the launcher's servers actually reach Minecraft.
    let _ = servers::write_servers_dat(&app, &game).await;

    let mut mods_list = vec![];

    if profile.use_celaris_client {
        // Prefer the latest auto-updated client jar from the server; fall back to
        // a locally built dev jar when offline / not yet published.
        let jar = clientmod::ensure_latest(&app)
            .await
            .or_else(|| find_celaris_jar(&profile.minecraft_version));
        if let Some(jar) = jar {
            mods_list.push(jar);
        }
        // Cross-version play: bundle ViaFabricPlus so the client can join servers
        // of almost any Minecraft version (1.7 → latest). Best-effort.
        if let Some(vfp) = clientmod::ensure_viafabricplus(&app, &profile.minecraft_version).await {
            mods_list.push(vfp);
        }
        // Bundle the performance/optimisation mod suite (Sodium, Lithium, …).
        for jar in clientmod::ensure_performance_mods(&app, &profile.minecraft_version).await {
            mods_list.push(jar);
        }
    }

    // Per-profile mod pool (each profile has its own mod list).
    let usermods = root.join("usermods").join(instance_slug(&profile.name));
    let _ = std::fs::create_dir_all(&usermods);

    let resolved = mods::resolve(&mods_list, Some(&usermods));
    let mod_jars = mods::load_order(&resolved);

    let mut config = profile_to_config(&profile, root, game, session, mod_jars);
    // Ensure a usable Java — download a managed JRE if the system has none that fits.
    let need = required_java(&profile.minecraft_version).await;
    config.java_path = java::resolve_or_install(&app, &config.java_path, need).await;
    // Direct "join server" launch from the launcher's server list.
    config.quick_play_multiplayer = server.and_then(|s| {
        let s = s.trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    });

    let reporter: Arc<dyn Reporter> = Arc::new(TauriReporter { app: app.clone() });

    launcher::install_and_launch(config, reporter)
        .await
        .map_err(|e| {
            let _ = app.emit("launch-error", &e);
            e.to_string()
        })
}

/// ---------------- MICROSOFT LOGIN ----------------

/// Online login via the legacy `login.live.com` OAuth flow with Microsoft's
/// first-party Minecraft client id (`00000000402b5328`) — pre-approved for the
/// Minecraft API, so it works without our own Azure app being on the allow list
/// (the same approach msmc / PrismLauncher's fallback use). Opens a login
/// webview, captures the redirect `code`, exchanges it for a session.
#[tauri::command]
async fn microsoft_login(app: AppHandle) -> Result<auth::OnlineLogin, String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(w) = app.get_webview_window("ms-login") {
        let _ = w.close();
    }

    let url: tauri::Url = auth::legacy_auth_url()
        .parse()
        .map_err(|_| "ungültige Auth-URL".to_string())?;
    let redirect = auth::legacy_redirect_prefix();

    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    let tx = std::sync::Mutex::new(Some(tx));

    let win = WebviewWindowBuilder::new(&app, "ms-login", WebviewUrl::External(url))
        .title("Bei Microsoft anmelden — Celaris")
        .inner_size(520.0, 720.0)
        .on_navigation(move |u| {
            if u.as_str().starts_with(redirect) {
                let code = u
                    .query_pairs()
                    .find(|(k, _)| k == "code")
                    .map(|(_, v)| v.to_string());
                if let Ok(mut g) = tx.lock() {
                    if let Some(sender) = g.take() {
                        let _ = sender.send(code);
                    }
                }
                return false;
            }
            true
        })
        .build()
        .map_err(|e| e.to_string())?;

    let captured = tokio::time::timeout(std::time::Duration::from_secs(300), rx).await;
    let _ = win.close();

    let code = match captured {
        Ok(Ok(Some(code))) => code,
        Ok(Ok(None)) => return Err("Login abgebrochen oder verweigert".to_string()),
        _ => return Err("Login-Zeit überschritten".to_string()),
    };

    auth::login_legacy(&code).await.map_err(|e| e.to_string())
}

/// Refreshes a stored Microsoft account so the user stays logged in across
/// restarts. Returns a fresh session + rotated refresh token to persist.
#[tauri::command]
async fn refresh_account(refresh_token: String) -> Result<auth::OnlineLogin, String> {
    auth::refresh_legacy(&refresh_token)
        .await
        .map_err(|e| e.to_string())
}

/// ---------------- VERSIONS ----------------

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    /// The launcher version actually running (baked in at build time).
    pub launcher: String,
    /// Name of the latest GitHub release (empty if unknown/offline).
    pub launcher_latest: String,
    /// Whether a newer launcher release than the running one exists.
    pub update_available: bool,
    /// GitHub release page to download the update from (for deb/rpm where the
    /// built-in updater can't self-replace).
    pub update_url: String,
    /// In-game client version — from the backend's `celaris/version.json`.
    pub client: String,
}

/// Normalises a version string for comparison ("v0.1.0" / " 0.1.0 " → "0.1.0").
fn norm_ver(s: &str) -> String {
    s.trim().trim_start_matches(['v', 'V']).trim().to_string()
}

/// Reports the launcher + in-game client versions (and whether a launcher update
/// is available) for display in the UI.
#[tauri::command]
async fn version_info() -> VersionInfo {
    let launcher = env!("CARGO_PKG_VERSION").to_string();
    let mut launcher_latest = String::new();
    let mut update_url = "https://github.com/CelarisClient/CelarisClient/releases/latest".to_string();
    let mut client = "—".to_string();

    if let Ok(http) = launcher::download::client() {
        // Latest GitHub release.
        if let Ok(resp) = http
            .get("https://api.github.com/repos/CelarisClient/CelarisClient/releases/latest")
            .header("User-Agent", "Celaris-Launcher")
            .header("Accept", "application/vnd.github+json")
            .timeout(std::time::Duration::from_secs(6))
            .send()
            .await
        {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                let name = v.get("name").and_then(|x| x.as_str()).filter(|s| !s.is_empty());
                let tag = v.get("tag_name").and_then(|x| x.as_str());
                if let Some(n) = name.or(tag) {
                    launcher_latest = n.to_string();
                }
                if let Some(u) = v.get("html_url").and_then(|x| x.as_str()) {
                    update_url = u.to_string();
                }
            }
        }
        // Client: backend version.json.
        if let Ok(resp) = http
            .get(format!("{CONTENT_BASE}/celaris/version.json"))
            .timeout(std::time::Duration::from_secs(6))
            .send()
            .await
        {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(ver) = v.get("version").and_then(|x| x.as_str()) {
                    client = ver.to_string();
                }
            }
        }
    }

    let update_available =
        !launcher_latest.is_empty() && norm_ver(&launcher_latest) != norm_ver(&launcher);

    VersionInfo { launcher, launcher_latest, update_available, update_url, client }
}

/// Opens a URL in the user's default browser (for the "download update" link).
#[tauri::command]
fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// ---------------- ENTRY ----------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Process-wide rustls crypto provider so the raw WebSocket (wss://) connects.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            detect_java,
            microsoft_login,
            refresh_account,
            version_info,
            open_external,
            coins::coins_packages,
            coins::coins_balance,
            coins::coins_checkout,
            coins::coins_capture,
            coins::coins_transfer,
            get_profiles,
            save_profiles,
            mods_dir,
            get_accounts,
            save_accounts,
            offline_session,
            launch,
            store::search_mods,
            store::install_mod,
            store::list_installed_mods,
            store::remove_mod,
            store::check_mod_updates,
            store::update_mod,
            store::search_resourcepacks,
            store::install_resourcepack,
            store::list_resourcepacks,
            store::remove_resourcepack,
            store::search_shaders,
            store::install_shader,
            store::list_shaders,
            store::remove_shader,
            store::profile_has_shaders,
            store::list_versions,
            packs::import_modpack,
            packs::export_celarispack,
            packs::list_global_modpacks,
            packs::install_global_modpack,
            packs::list_server_modpacks,
            packs::install_server_modpack,
            skins::grab_skin,
            skins::import_skin,
            skins::list_wardrobe,
            skins::remove_skin,
            skins::apply_skin,
            servers::list_partner_servers,
            servers::ping_server,
            servers::get_servers,
            servers::save_servers,
            servers::sync_servers,
            news::fetch_news,
            social::social_connect,
            social::social_send_chat,
            social::social_send_dm,
            social::social_set_presence,
            social::social_share_screenshot,
            social::social_friend_add,
            social::social_friend_accept,
            social::social_friend_remove,
            social::social_friends_list,
            spotify::spotify_login,
            spotify::spotify_status,
            spotify::spotify_now_playing,
            spotify::spotify_control,
            #[cfg(feature = "admin")]
            admin::admin_set_token,
            #[cfg(feature = "admin")]
            admin::admin_has_token,
            #[cfg(feature = "admin")]
            admin::admin_grant,
            #[cfg(feature = "admin")]
            admin::admin_upload
        ])
        .run(tauri::generate_context!())
        .expect("failed to run app");
}