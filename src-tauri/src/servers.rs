//! Server list: partner servers (admin-managed, fetched over HTTP, pinned on top)
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

/// Best-effort fetch of partner servers from the admin content host. Returns an
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

/// Pings a server's live status. Robust by design:
///   1. A **direct TCP Server-List-Ping** to `host:port` (ground truth, no
///      third-party rate limits) — this is what makes Hypixel & co. report online
///      reliably.
///   2. Falls back to the **mcstatus.io** API only if the direct ping fails (e.g.
///      SRV-only domains that don't answer on the default port).
/// Returns `online:false` only when BOTH fail.
#[tauri::command]
pub async fn ping_server(address: String) -> ServerStatus {
    let addr = address.trim();
    if addr.is_empty() {
        return ServerStatus::default();
    }

    // 1. Direct SLP.
    let (host, port) = parse_host_port(addr);
    if let Some(v) = slp_status(&host, port).await {
        return status_from_slp(&v);
    }

    // 2. Fallback: mcstatus.io (handles SRV records + favicon).
    let Ok(client) = download::client() else {
        return ServerStatus::default();
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

/// Splits `host` / `host:port`, defaulting to the Minecraft port 25565.
fn parse_host_port(addr: &str) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => (h.to_string(), p.trim().parse().unwrap_or(25565)),
        _ => (addr.to_string(), 25565),
    }
}

fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut val = value as u32;
    loop {
        let mut b = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if val == 0 {
            break;
        }
    }
}

async fn read_varint<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> std::io::Result<i32> {
    use tokio::io::AsyncReadExt;
    let mut num: u32 = 0;
    let mut shift = 0;
    loop {
        let b = r.read_u8().await?;
        num |= ((b & 0x7F) as u32) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 35 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "varint too long"));
        }
    }
    Ok(num as i32)
}

/// Performs a modern (1.7+) Server-List-Ping handshake and returns the status JSON.
async fn slp_status(host: &str, port: u16) -> Option<serde_json::Value> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fut = async {
        let mut stream = tokio::net::TcpStream::connect((host, port)).await.ok()?;

        // Handshake packet (id 0x00): protocol -1, host, port, next-state 1 (status).
        let mut hs = Vec::new();
        write_varint(&mut hs, 0x00);
        write_varint(&mut hs, -1);
        write_varint(&mut hs, host.len() as i32);
        hs.extend_from_slice(host.as_bytes());
        hs.extend_from_slice(&port.to_be_bytes());
        write_varint(&mut hs, 1);

        let mut packet = Vec::new();
        write_varint(&mut packet, hs.len() as i32);
        packet.extend_from_slice(&hs);
        // Status request (length 1, id 0x00).
        write_varint(&mut packet, 1);
        write_varint(&mut packet, 0x00);
        stream.write_all(&packet).await.ok()?;

        // Response: [packet length][packet id][json length][json].
        let _packet_len = read_varint(&mut stream).await.ok()?;
        let _packet_id = read_varint(&mut stream).await.ok()?;
        let json_len = read_varint(&mut stream).await.ok()?;
        if json_len <= 0 || json_len > 2_000_000 {
            return None;
        }
        let mut buf = vec![0u8; json_len as usize];
        stream.read_exact(&mut buf).await.ok()?;
        serde_json::from_slice::<serde_json::Value>(&buf).ok()
    };
    tokio::time::timeout(Duration::from_secs(5), fut).await.ok().flatten()
}

/// Builds a [`ServerStatus`] from a raw SLP status JSON. A response at all means
/// the server is online.
fn status_from_slp(v: &serde_json::Value) -> ServerStatus {
    let players = v.get("players");
    ServerStatus {
        online: true,
        players: players.and_then(|p| p.get("online")).and_then(|n| n.as_i64()).unwrap_or(0),
        max: players.and_then(|p| p.get("max")).and_then(|n| n.as_i64()).unwrap_or(0),
        icon: v.get("favicon").and_then(|f| f.as_str()).map(|s| s.to_string()),
        motd: strip_legacy(&flatten_desc(v.get("description").unwrap_or(&serde_json::Value::Null))),
        version: v
            .get("version")
            .and_then(|ver| ver.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| strip_legacy(s)),
    }
}

/// Flattens a chat-component MOTD (string, or {text, extra:[…]}) into plain text.
fn flatten_desc(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(o) => {
            let mut s = String::new();
            if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                s.push_str(t);
            }
            if let Some(extra) = o.get("extra").and_then(|e| e.as_array()) {
                for e in extra {
                    s.push_str(&flatten_desc(e));
                }
            }
            s
        }
        serde_json::Value::Array(arr) => arr.iter().map(flatten_desc).collect(),
        _ => String::new(),
    }
}

/// Strips legacy `§x` colour/format codes.
fn strip_legacy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
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
