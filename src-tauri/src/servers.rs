//! Server list: partner servers (fetched over HTTP, pinned on top)
//! + the user's own servers, written into Minecraft's `servers.dat` so they show
//! up in-game. The launcher only ever fetches HTTP/JSON, never a database.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::launcher::download;
use crate::{Profile, CONTENT_BASE};

#[derive(Serialize, Deserialize, Clone)]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub partner: bool,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional base64 PNG (64×64) server icon.
    #[serde(default)]
    pub icon: Option<String>,
    /// Optional wide promo banner image URL (partners only).
    #[serde(default)]
    pub banner: Option<String>,
}

fn servers_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("servers.json"))
}

/// Best-effort fetch of partner servers from the content host. Returns an
/// empty list (not an error) when unreachable so the UI degrades gracefully.
async fn fetch_partners() -> Vec<ServerEntry> {
    let client = match download::client() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let result = client
        .get(format!("{CONTENT_BASE}/partners.json"))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status());
    let resp = match result {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    match resp.json::<Vec<ServerEntry>>().await {
        Ok(list) => list
            .into_iter()
            .map(|mut s| {
                s.partner = true;
                s
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// API base (the content host minus the `/content` suffix), e.g.
/// `https://api.celarisclient.de`.
fn api_base() -> String {
    CONTENT_BASE.trim_end_matches("/content").to_string()
}

#[derive(Deserialize)]
struct ApiPartner {
    name: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    icon_url: String,
    #[serde(default)]
    banner_url: String,
}

/// Partners from the DB-backed `/api/partners` endpoint. `pinned_only` keeps just
/// the servers that should be force-injected into every player's list.
async fn fetch_api_partners(pinned_only: bool) -> Vec<ServerEntry> {
    let client = match download::client() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let result = client
        .get(format!("{}/api/partners", api_base()))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status());
    let resp = match result {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    match resp.json::<Vec<ApiPartner>>().await {
        Ok(list) => list
            .into_iter()
            .filter(|p| !p.address.trim().is_empty() && (!pinned_only || p.pinned))
            .map(|p| ServerEntry {
                name: p.name,
                address: p.address,
                partner: true,
                description: if p.description.is_empty() { None } else { Some(p.description) },
                icon: if p.icon_url.is_empty() { None } else { Some(p.icon_url) },
                banner: if p.banner_url.is_empty() { None } else { Some(p.banner_url) },
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Merges entries, keeping the first occurrence of each address (case-insensitive).
fn dedupe_by_address(entries: Vec<ServerEntry>) -> Vec<ServerEntry> {
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|s| seen.insert(s.address.to_lowercase()))
        .collect()
}

/// Live status + banner (favicon) of a Minecraft server, for the server list.
#[derive(Serialize, Default)]
pub struct ServerStatus {
    pub online: bool,
    pub players: i64,
    pub max: i64,
    /// `data:image/png;base64,…` server icon (banner), if the server sets one.
    pub icon: Option<String>,
    pub motd: String,
    pub version: Option<String>,
}

#[derive(Deserialize)]
struct McStatusResp {
    #[serde(default)]
    online: bool,
    #[serde(default)]
    players: Option<McStatusPlayers>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    motd: Option<McStatusMotd>,
    #[serde(default)]
    version: Option<McStatusVersion>,
}
#[derive(Deserialize)]
struct McStatusPlayers { #[serde(default)] online: i64, #[serde(default)] max: i64 }
#[derive(Deserialize)]
struct McStatusMotd { #[serde(default)] clean: String }
#[derive(Deserialize)]
struct McStatusVersion { #[serde(default)] name_clean: Option<String> }

/// Pings a server via the public mcstatus.io API (handles SRV + the SLP protocol
/// + favicon for us). Best-effort: returns `online:false` on any failure.
#[tauri::command]
pub async fn ping_server(address: String) -> ServerStatus {
    let addr = address.trim();
    if addr.is_empty() {
        return ServerStatus::default();
    }
    let client = match download::client() {
        Ok(c) => c,
        Err(_) => return ServerStatus::default(),
    };
    let url = format!("https://api.mcstatus.io/v2/status/java/{addr}");
    let resp = client.get(&url).timeout(Duration::from_secs(8)).send().await;
    let Ok(resp) = resp.and_then(|r| r.error_for_status()) else {
        return ServerStatus::default();
    };
    match resp.json::<McStatusResp>().await {
        Ok(s) => ServerStatus {
            online: s.online,
            players: s.players.as_ref().map(|p| p.online).unwrap_or(0),
            max: s.players.as_ref().map(|p| p.max).unwrap_or(0),
            icon: s.icon,
            motd: s.motd.map(|m| m.clean).unwrap_or_default(),
            version: s.version.and_then(|v| v.name_clean),
        },
        Err(_) => ServerStatus::default(),
    }
}

/// servers.dat wants the raw base64 PNG (no `data:image/png;base64,` prefix).
fn strip_data_url(icon: &str) -> String {
    match icon.split_once("base64,") {
        Some((_, b64)) => b64.trim().to_string(),
        None => icon.trim().to_string(),
    }
}

/// Fetches a server's favicon as raw base64 (for embedding in servers.dat so the
/// in-game multiplayer list shows the banner immediately). Best-effort.
async fn fetch_server_icon(client: &reqwest::Client, addr: &str) -> Option<String> {
    let url = format!("https://api.mcstatus.io/v2/status/java/{}", addr.trim());
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let s: McStatusResp = resp.json().await.ok()?;
    s.icon.map(|i| strip_data_url(&i))
}

#[tauri::command]
pub async fn list_partner_servers() -> Result<Vec<ServerEntry>, String> {
    let mut all = fetch_partners().await;
    all.extend(fetch_api_partners(false).await);
    Ok(dedupe_by_address(all))
}

#[tauri::command]
pub fn get_servers(app: AppHandle) -> Result<Vec<ServerEntry>, String> {
    let path = servers_path(&app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_servers(app: AppHandle, servers: Vec<ServerEntry>) -> Result<(), String> {
    let path = servers_path(&app)?;
    let json = serde_json::to_string_pretty(&servers).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// servers.dat (NBT)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ServersDat {
    servers: Vec<ServerNbt>,
}

#[derive(Serialize)]
struct ServerNbt {
    name: String,
    ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
}

/// The game dir Minecraft actually runs in for a profile — MUST match the launch
/// logic in `lib.rs` (`instances/<slug>`), else servers.dat lands in the wrong
/// place and never shows up in-game.
fn profile_game_dir(app: &AppHandle, profile: &Profile) -> Result<PathBuf, String> {
    if profile.game_dir.trim().is_empty() {
        Ok(app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("minecraft")
            .join("instances")
            .join(crate::instance_slug(&profile.name)))
    } else {
        Ok(PathBuf::from(&profile.game_dir))
    }
}

/// Writes partner + user servers (partners first) into a game dir's
/// `servers.dat`. Reused by both the manual sync command and the launch flow.
pub async fn write_servers_dat(app: &AppHandle, game_dir: &std::path::Path) -> Result<usize, String> {
    std::fs::create_dir_all(game_dir).map_err(|e| e.to_string())?;

    // Pinned partners (Lunar-style, auto-injected) first, then user servers.
    let mut merged = fetch_partners().await;
    merged.extend(fetch_api_partners(true).await);
    merged.extend(get_servers(app.clone())?);
    let merged = dedupe_by_address(merged);

    // Resolve each server's banner (existing partner icon → live favicon) so the
    // in-game multiplayer list shows banners just like the launcher does.
    let client = download::client().ok();
    let icons: Vec<Option<String>> = futures_util::future::join_all(merged.iter().map(|s| {
        let client = client.clone();
        let existing = s.icon.clone();
        let addr = s.address.clone();
        async move {
            if let Some(ic) = existing {
                return Some(strip_data_url(&ic));
            }
            match client {
                Some(c) => fetch_server_icon(&c, &addr).await,
                None => None,
            }
        }
    }))
    .await;

    let dat = ServersDat {
        servers: merged
            .iter()
            .zip(icons)
            .map(|(s, icon)| ServerNbt {
                name: s.name.clone(),
                ip: s.address.clone(),
                icon,
            })
            .collect(),
    };

    let bytes = fastnbt::to_bytes(&dat).map_err(|e| e.to_string())?;
    std::fs::write(game_dir.join("servers.dat"), bytes).map_err(|e| e.to_string())?;
    Ok(merged.len())
}

/// Writes partner + user servers (partners first) into the profile's
/// `servers.dat` so Minecraft shows them in the multiplayer list.
#[tauri::command]
pub async fn sync_servers(app: AppHandle, profile: Profile) -> Result<usize, String> {
    let game_dir = profile_game_dir(&app, &profile)?;
    write_servers_dat(&app, &game_dir).await
}
