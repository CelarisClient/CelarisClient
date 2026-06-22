//! Launcher coin shop: talks to the Celaris server's coin API on the user's
//! behalf. The player's Microsoft access token is exchanged for a short session
//! token (`/api/auth/verify`), which authorises the owner-scoped coin endpoints.

use serde_json::Value;

use crate::launcher::download;

const API: &str = "https://api.celarisclient.de";

/// Exchanges the Microsoft access token for a Celaris session (Bearer) token.
async fn session_token(http: &reqwest::Client, access_token: &str) -> Result<String, String> {
    let resp = http
        .post(format!("{API}/api/auth/verify"))
        .json(&serde_json::json!({ "access_token": access_token }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    v["token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Konto konnte nicht verifiziert werden (Microsoft-Login nötig)".to_string())
}

/// Public list of coin packages (no auth).
#[tauri::command]
pub async fn coins_packages() -> Result<Value, String> {
    let http = download::client().map_err(|e| e.to_string())?;
    http.get(format!("{API}/api/coin-packages"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

/// Current coin balance for the signed-in player.
#[tauri::command]
pub async fn coins_balance(access_token: String, username: String) -> Result<i64, String> {
    let http = download::client().map_err(|e| e.to_string())?;
    let tok = session_token(&http, &access_token).await?;
    let v: Value = http
        .get(format!("{API}/api/coins/{username}"))
        .bearer_auth(tok)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(v["coins"].as_i64().unwrap_or(0))
}

/// Starts a Lemon Squeezy purchase; returns `{ url }`. The launcher opens the url,
/// the user pays, and Lemon Squeezy's signed webhook credits the coins server-side.
/// The launcher just re-reads the balance afterwards (`coins_balance`).
#[tauri::command]
pub async fn coins_checkout(access_token: String, username: String, package: String) -> Result<Value, String> {
    let http = download::client().map_err(|e| e.to_string())?;
    let tok = session_token(&http, &access_token).await?;
    let resp = http
        .post(format!("{API}/api/coins/checkout"))
        .bearer_auth(tok)
        .json(&serde_json::json!({ "username": username, "package": package }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(resp.text().await.unwrap_or_else(|_| "Fehler".into()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Gifts coins to another player; returns `{ ok, coins }`.
#[tauri::command]
pub async fn coins_transfer(access_token: String, username: String, to: String, amount: i64) -> Result<Value, String> {
    let http = download::client().map_err(|e| e.to_string())?;
    let tok = session_token(&http, &access_token).await?;
    let resp = http
        .post(format!("{API}/api/coins/transfer"))
        .bearer_auth(tok)
        .json(&serde_json::json!({ "username": username, "to": to, "amount": amount }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(resp.text().await.unwrap_or_else(|_| "Fehler".into()));
    }
    resp.json().await.map_err(|e| e.to_string())
}
