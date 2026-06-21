//! Skin wardrobe: grab a player's skin by name or UUID (public Mojang APIs),
//! import skins from local PNGs, store them locally and list them for preview.
//! Applying a skin to the user's own account needs an authenticated session and
//! is therefore gated until Microsoft login is approved.

use std::path::PathBuf;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::launcher::download;

fn skins_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("skins");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "skin".to_string() } else { s }
}

/// A wardrobe entry: metadata + the skin PNG as a base64 data payload.
#[derive(Serialize)]
pub struct SkinInfo {
    pub id: String,
    pub name: String,
    pub uuid: String,
    pub png_base64: String,
}

#[derive(Serialize, Deserialize)]
struct SkinMeta {
    name: String,
    uuid: String,
}

// --- Mojang API shapes ---
#[derive(Deserialize)]
struct MojangProfile {
    id: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    name: String,
    properties: Vec<SessionProp>,
}

#[derive(Deserialize)]
struct SessionProp {
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: TextureMap,
}

#[derive(Deserialize)]
struct TextureMap {
    #[serde(rename = "SKIN")]
    skin: Option<TextureUrl>,
}

#[derive(Deserialize)]
struct TextureUrl {
    url: String,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn save_entry(app: &AppHandle, id: &str, name: &str, uuid: &str, png: &[u8]) -> Result<(), String> {
    let dir = skins_dir(app)?;
    std::fs::write(dir.join(format!("{id}.png")), png).map_err(|e| e.to_string())?;
    let meta = serde_json::to_string(&SkinMeta {
        name: name.to_string(),
        uuid: uuid.to_string(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{id}.json")), meta).map_err(|e| e.to_string())?;
    Ok(())
}

/// Grab a skin by player name or UUID (the "skin stealer").
#[tauri::command]
pub async fn grab_skin(app: AppHandle, query: String) -> Result<SkinInfo, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let stripped = query.replace('-', "");
    let looks_like_uuid = stripped.len() == 32 && stripped.chars().all(|c| c.is_ascii_hexdigit());

    let uuid = if looks_like_uuid {
        stripped
    } else {
        let profile: MojangProfile = client
            .get(format!(
                "https://api.mojang.com/users/profiles/minecraft/{query}"
            ))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|_| format!("Spieler „{query}\" nicht gefunden"))?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        profile.id
    };

    let session: SessionProfile = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"
        ))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|_| "Profil nicht gefunden".to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let value = session
        .properties
        .first()
        .ok_or("Profil hat keine Texturen")?
        .value
        .clone();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|e| e.to_string())?;
    let textures: Textures = serde_json::from_slice(&decoded).map_err(|e| e.to_string())?;
    let skin_url = textures
        .textures
        .skin
        .ok_or("Spieler nutzt den Standard-Skin")?
        .url;

    let png = download::get_bytes(&client, &skin_url)
        .await
        .map_err(|e| e.to_string())?;

    let id = format!("{}-{}", sanitize(&session.name), &uuid[..8]);
    save_entry(&app, &id, &session.name, &uuid, &png)?;

    Ok(SkinInfo {
        id,
        name: session.name,
        uuid,
        png_base64: b64(&png),
    })
}

/// Import a skin from a local PNG file.
#[tauri::command]
pub fn import_skin(app: AppHandle, path: String, label: String) -> Result<SkinInfo, String> {
    let png = std::fs::read(&path).map_err(|e| e.to_string())?;
    if png.get(1..4) != Some(b"PNG") {
        return Err("Datei ist kein PNG".to_string());
    }
    let name = if label.trim().is_empty() {
        PathBuf::from(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Skin".to_string())
    } else {
        label
    };
    let id = format!("{}-import", sanitize(&name));
    save_entry(&app, &id, &name, "", &png)?;
    Ok(SkinInfo {
        id,
        name,
        uuid: String::new(),
        png_base64: b64(&png),
    })
}

#[tauri::command]
pub fn list_wardrobe(app: AppHandle) -> Result<Vec<SkinInfo>, String> {
    let dir = skins_dir(&app)?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "png").unwrap_or(false) {
                let id = path.file_stem().unwrap().to_string_lossy().to_string();
                let png = match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let meta: SkinMeta = std::fs::read_to_string(dir.join(format!("{id}.json")))
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(SkinMeta {
                        name: id.clone(),
                        uuid: String::new(),
                    });
                out.push(SkinInfo {
                    id,
                    name: meta.name,
                    uuid: meta.uuid,
                    png_base64: b64(&png),
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[tauri::command]
pub fn remove_skin(app: AppHandle, id: String) -> Result<(), String> {
    let dir = skins_dir(&app)?;
    let png = dir.join(format!("{id}.png"));
    if png.parent() != Some(dir.as_path()) {
        return Err("ungültige ID".to_string());
    }
    let _ = std::fs::remove_file(dir.join(format!("{id}.json")));
    std::fs::remove_file(png).map_err(|e| e.to_string())
}

/// Applies a wardrobe skin to the player's Microsoft account via the Minecraft
/// services API (multipart upload). Needs the account's Minecraft access token.
#[tauri::command]
pub async fn apply_skin(
    app: AppHandle,
    id: String,
    access_token: String,
    slim: bool,
) -> Result<(), String> {
    if access_token.is_empty() || access_token == "0" {
        return Err("Skin anwenden benötigt einen Microsoft-Account.".to_string());
    }
    let dir = skins_dir(&app)?;
    let path = dir.join(format!("{id}.png"));
    if path.parent() != Some(dir.as_path()) {
        return Err("ungültiger Pfad".to_string());
    }
    let png = std::fs::read(&path).map_err(|e| e.to_string())?;

    let client = download::client().map_err(|e| e.to_string())?;
    let part = reqwest::multipart::Part::bytes(png)
        .file_name("skin.png")
        .mime_str("image/png")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("variant", if slim { "slim" } else { "classic" })
        .part("file", part);

    let res = client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(&access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if res.status().is_success() {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!(
            "Skin-Upload fehlgeschlagen ({status}): {}",
            body.chars().take(200).collect::<String>()
        ))
    }
}
