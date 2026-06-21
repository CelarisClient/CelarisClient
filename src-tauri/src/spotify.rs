//! Spotify integration: Authorization-Code-with-PKCE login (no client secret),
//! now-playing and playback controls. Tokens persist in the app config dir.
//!
//! Requires a Spotify Developer app whose Redirect URI is
//! `http://127.0.0.1:8888/callback`. Set its client id via `SPOTIFY_CLIENT_ID`
//! (or hardcode `CLIENT_ID` below for shipping).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::launcher::download;

const CLIENT_ID: &str = "YOUR_SPOTIFY_CLIENT_ID";
const REDIRECT: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &str = "user-read-currently-playing user-read-playback-state user-modify-playback-state";

static TOKENS: Mutex<Option<Tokens>> = Mutex::new(None);
/// Guards against a second concurrent login holding the callback port.
static LOGIN_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Binds the callback port with SO_REUSEADDR and an accept timeout so it is never
/// left stuck (e.g. when the user closes the browser without authorizing).
fn bind_callback() -> Result<TcpListener, String> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None).map_err(|e| e.to_string())?;
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(90)))
        .map_err(|e| e.to_string())?;
    let addr: SocketAddr = "127.0.0.1:8888".parse().unwrap();
    socket.bind(&addr.into()).map_err(|e| format!("Callback-Port 8888: {e}"))?;
    socket.listen(1).map_err(|e| e.to_string())?;
    Ok(socket.into())
}

#[derive(Serialize, Deserialize, Clone)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}

#[derive(Serialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album_art: Option<String>,
    pub is_playing: bool,
    pub progress_ms: u64,
    pub duration_ms: u64,
}

fn client_id() -> String {
    std::env::var("SPOTIFY_CLIENT_ID").unwrap_or_else(|_| CLIENT_ID.to_string())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn tokens_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("spotify.json"))
}

fn load_tokens(app: &AppHandle) {
    if TOKENS.lock().unwrap().is_some() {
        return;
    }
    if let Ok(path) = tokens_path(app) {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(t) = serde_json::from_str::<Tokens>(&data) {
                *TOKENS.lock().unwrap() = Some(t);
            }
        }
    }
}

fn store_tokens(app: &AppHandle, tokens: Tokens) {
    if let Ok(path) = tokens_path(app) {
        let _ = std::fs::write(path, serde_json::to_string(&tokens).unwrap_or_default());
    }
    *TOKENS.lock().unwrap() = Some(tokens);
}

// ---------------------------------------------------------------------------
// Login (PKCE)
// ---------------------------------------------------------------------------

fn pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    let _ = getrandom::getrandom(&mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    (verifier, challenge)
}

/// Blocks until Spotify redirects to the local callback, returns the auth code.
fn wait_for_code(listener: TcpListener) -> Result<String, String> {
    let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    let request = String::from_utf8_lossy(&buf[..n]);
    // GET /callback?code=XYZ&... HTTP/1.1
    let code = request
        .split_whitespace()
        .nth(1)
        .and_then(|path| path.split("code=").nth(1))
        .map(|rest| rest.split('&').next().unwrap_or("").to_string())
        .filter(|c| !c.is_empty())
        .ok_or("Kein Code in der Antwort")?;

    let body = "<html><body style='font-family:sans-serif;background:#0a0c12;color:#e9edf6;text-align:center;padding-top:80px'><h2>Celaris × Spotify verbunden ✓</h2><p>Du kannst dieses Fenster schließen.</p></body></html>";
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    Ok(code)
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
}

struct LoginGuard;
impl Drop for LoginGuard {
    fn drop(&mut self) {
        LOGIN_ACTIVE.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn spotify_login(app: AppHandle) -> Result<(), String> {
    if LOGIN_ACTIVE.swap(true, Ordering::SeqCst) {
        return Err("Spotify-Login läuft bereits — schließe ggf. das Browser-Fenster.".into());
    }
    let _guard = LoginGuard;

    let (verifier, challenge) = pkce();
    let auth_url = format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri={}&code_challenge_method=S256&code_challenge={}&scope={}",
        client_id(),
        enc(REDIRECT),
        challenge,
        enc(SCOPES)
    );

    // Bind the callback listener BEFORE opening the browser to avoid a race.
    let listener = bind_callback()?;
    let _ = app.emit("spotify-auth-url", auth_url);

    let code = tokio::task::spawn_blocking(move || wait_for_code(listener))
        .await
        .map_err(|e| e.to_string())??;

    let client = download::client().map_err(|e| e.to_string())?;
    let resp: TokenResp = client
        .post("https://accounts.spotify.com/api/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", REDIRECT),
            ("client_id", &client_id()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Token-Tausch fehlgeschlagen: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    store_tokens(
        &app,
        Tokens {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token.unwrap_or_default(),
            expires_at: now() + resp.expires_in,
        },
    );
    let _ = app.emit("spotify-connected", ());
    Ok(())
}

async fn ensure_access_token(app: &AppHandle) -> Result<String, String> {
    load_tokens(app);
    let current = TOKENS.lock().unwrap().clone();
    let tokens = current.ok_or("Nicht mit Spotify verbunden")?;
    if tokens.expires_at > now() + 15 {
        return Ok(tokens.access_token);
    }

    // Refresh.
    let client = download::client().map_err(|e| e.to_string())?;
    let resp: TokenResp = client
        .post("https://accounts.spotify.com/api/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", tokens.refresh_token.as_str()),
            ("client_id", &client_id()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let new = Tokens {
        access_token: resp.access_token.clone(),
        refresh_token: resp.refresh_token.unwrap_or(tokens.refresh_token),
        expires_at: now() + resp.expires_in,
    };
    store_tokens(app, new);
    Ok(resp.access_token)
}

#[tauri::command]
pub fn spotify_status(app: AppHandle) -> bool {
    load_tokens(&app);
    TOKENS.lock().unwrap().is_some()
}

// ---------------------------------------------------------------------------
// Now playing + controls
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Playing {
    #[serde(default)]
    is_playing: bool,
    #[serde(default)]
    progress_ms: u64,
    #[serde(default)]
    item: Option<PlayItem>,
}

#[derive(Deserialize)]
struct PlayItem {
    name: String,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    artists: Vec<NamePart>,
    album: Album,
}

#[derive(Deserialize)]
struct NamePart {
    name: String,
}

#[derive(Deserialize)]
struct Album {
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[tauri::command]
pub async fn spotify_now_playing(app: AppHandle) -> Result<Option<NowPlaying>, String> {
    let token = ensure_access_token(&app).await?;
    let client = download::client().map_err(|e| e.to_string())?;
    let resp = client
        .get("https://api.spotify.com/v1/me/player/currently-playing")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    let playing: Playing = resp
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let Some(item) = playing.item else {
        return Ok(None);
    };
    Ok(Some(NowPlaying {
        title: item.name,
        artist: item
            .artists
            .into_iter()
            .map(|a| a.name)
            .collect::<Vec<_>>()
            .join(", "),
        album_art: item.album.images.into_iter().next().map(|i| i.url),
        is_playing: playing.is_playing,
        progress_ms: playing.progress_ms,
        duration_ms: item.duration_ms,
    }))
}

#[tauri::command]
pub async fn spotify_control(app: AppHandle, action: String) -> Result<(), String> {
    let token = ensure_access_token(&app).await?;
    let client = download::client().map_err(|e| e.to_string())?;
    let base = "https://api.spotify.com/v1/me/player";
    let req = match action.as_str() {
        "play" => client.put(format!("{base}/play")).bearer_auth(token).header("Content-Length", "0"),
        "pause" => client.put(format!("{base}/pause")).bearer_auth(token).header("Content-Length", "0"),
        "next" => client.post(format!("{base}/next")).bearer_auth(token).header("Content-Length", "0"),
        "previous" => client.post(format!("{base}/previous")).bearer_auth(token).header("Content-Length", "0"),
        other => return Err(format!("Unbekannte Aktion: {other}")),
    };
    req.send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}
