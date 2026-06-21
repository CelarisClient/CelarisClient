//! Social client: a WebSocket connection to the Celaris client-connection server.
//! Sends presence/chat/screenshots and re-emits everything the server broadcasts
//! to the frontend as `social-*` Tauri events.

use std::sync::Mutex;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::Profile;

/// Outgoing-message sender for the active connection (None when disconnected).
static OUTGOING: Mutex<Option<mpsc::UnboundedSender<String>>> = Mutex::new(None);

fn social_url() -> String {
    std::env::var("CELARIS_SOCIAL_URL").unwrap_or_else(|_| "wss://api.celarisclient.de/ws".into())
}

fn send_raw(json: String) -> Result<(), String> {
    let guard = OUTGOING.lock().unwrap();
    match guard.as_ref() {
        Some(tx) => tx.send(json).map_err(|_| "Verbindung getrennt".to_string()),
        None => Err("Nicht verbunden".to_string()),
    }
}

#[tauri::command]
pub async fn social_connect(
    app: AppHandle,
    username: String,
    uuid: Option<String>,
    access_token: Option<String>,
) -> Result<(), String> {
    let (ws, _) = connect_async(social_url())
        .await
        .map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let (mut write, mut read) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    *OUTGOING.lock().unwrap() = Some(tx.clone());

    // Identify ourselves. The access token lets the server verify the identity so
    // nobody can impersonate another player's social session.
    let _ = tx.send(
        serde_json::json!({ "type": "hello", "username": username, "uuid": uuid, "token": access_token })
            .to_string(),
    );

    // Writer task: forward queued outgoing messages to the socket.
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Reader task: re-emit server broadcasts as Tauri events.
    let app2 = app.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            if let Message::Text(text) = msg {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    let event = match kind {
                        "presence" => "social-presence",
                        "chat" => "social-chat",
                        "dm" => "social-dm",
                        "screenshot" => "social-screenshot",
                        "friends" => "social-friends",
                        "friend_request" => "social-friend-request",
                        "system" => "social-system",
                        _ => "social-other",
                    };
                    let _ = app2.emit(event, value);
                }
            }
        }
        *OUTGOING.lock().unwrap() = None;
        let _ = app2.emit("social-disconnected", ());
    });

    Ok(())
}

#[tauri::command]
pub fn social_send_chat(text: String) -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "chat", "text": text }).to_string())
}

/// Sends a private message to a single friend.
#[tauri::command]
pub fn social_send_dm(to: String, text: String) -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "dm", "to": to, "text": text }).to_string())
}

#[tauri::command]
pub fn social_friend_add(username: String) -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "friend_add", "username": username }).to_string())
}

#[tauri::command]
pub fn social_friend_accept(username: String) -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "friend_accept", "username": username }).to_string())
}

#[tauri::command]
pub fn social_friend_remove(username: String) -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "friend_remove", "username": username }).to_string())
}

#[tauri::command]
pub fn social_friends_list() -> Result<(), String> {
    send_raw(serde_json::json!({ "type": "friends_list" }).to_string())
}

#[tauri::command]
pub fn social_set_presence(
    status: String,
    server: Option<String>,
    playtime_secs: u64,
) -> Result<(), String> {
    send_raw(
        serde_json::json!({
            "type": "status",
            "status": status,
            "server": server,
            "playtime_secs": playtime_secs,
        })
        .to_string(),
    )
}

/// Shares the most recent screenshot from the profile's `screenshots/` folder.
#[tauri::command]
pub fn social_share_screenshot(app: AppHandle, profile: Profile) -> Result<String, String> {
    let game_dir = if profile.game_dir.trim().is_empty() {
        app.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("minecraft")
            .join("instance")
    } else {
        std::path::PathBuf::from(&profile.game_dir)
    };
    let shots = game_dir.join("screenshots");

    let newest = std::fs::read_dir(&shots)
        .map_err(|_| "Keine Screenshots gefunden".to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
        .ok_or("Keine Screenshots gefunden")?;

    let bytes = std::fs::read(&newest).map_err(|e| e.to_string())?;
    if bytes.len() > 6_000_000 {
        return Err("Screenshot zu groß (>6 MB)".to_string());
    }
    let name = newest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "screenshot.png".into());
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);

    send_raw(serde_json::json!({ "type": "screenshot", "name": name, "data": data }).to_string())?;
    Ok(name)
}
