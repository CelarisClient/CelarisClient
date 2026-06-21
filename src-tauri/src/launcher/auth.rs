//! Authentication layer.
//!
//! Produces a validated [`Session`] (uuid, token, username, user-type) that the
//! bridge places into `CelarisLaunchConfig`. This layer is independent of the launch
//! engine — the pipeline only ever consumes the finished `Session`, never the auth
//! flow itself.
//!
//! Two providers implement the [`AuthProvider`] abstraction:
//!   * [`OfflineProvider`] — deterministic offline session (the always-available fallback).
//!   * [`MicrosoftProvider`] — Microsoft device-code OAuth → Xbox Live → XSTS →
//!     Minecraft services → profile.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A validated player session handed to the launch engine.
///
/// `Deserialize` lets the UI pass a Microsoft session (obtained via
/// `microsoft_login`) back into the `launch` command — pure wiring, no new logic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// "msa" for Microsoft accounts, "legacy" for offline.
    pub user_type: String,
}

impl Session {
    /// Deterministic offline session: md5-based UUID, dummy token. Always succeeds.
    pub fn offline(username: &str) -> Session {
        Session {
            username: username.to_string(),
            uuid: offline_uuid(username),
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
        }
    }
}

/// Offline (md5-based) player UUID, matching the vanilla offline-mode scheme.
pub fn offline_uuid(name: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{name}"));
    let mut b = digest.0;
    b[6] = (b[6] & 0x0f) | 0x30; // version 3
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

/// Structured authentication failures (separate from the launch pipeline's errors,
/// since auth runs before a launch is ever configured).
#[derive(Debug, Clone)]
pub enum AuthError {
    Network(String),
    DeviceCodeRequestFailed(String),
    AuthorizationDeclined,
    AuthorizationTimedOut,
    XboxLiveFailed(String),
    XstsFailed(String),
    NoMinecraftAccount,
    ProfileFailed(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Network(m) => write!(f, "network error: {m}"),
            AuthError::DeviceCodeRequestFailed(m) => write!(f, "device code request failed: {m}"),
            AuthError::AuthorizationDeclined => write!(f, "authorization declined by user"),
            AuthError::AuthorizationTimedOut => write!(f, "authorization timed out"),
            AuthError::XboxLiveFailed(m) => write!(f, "Xbox Live auth failed: {m}"),
            AuthError::XstsFailed(m) => write!(f, "XSTS auth failed: {m}"),
            AuthError::NoMinecraftAccount => write!(f, "account owns no Minecraft profile"),
            AuthError::ProfileFailed(m) => write!(f, "profile lookup failed: {m}"),
        }
    }
}

/// The authentication abstraction. Every provider yields a validated [`Session`].
///
/// Providers are used through concrete types (not trait objects), so the
/// `async_fn_in_trait` Send caveat does not apply here.
#[allow(async_fn_in_trait)]
pub trait AuthProvider {
    async fn authenticate(&self) -> Result<Session, AuthError>;
}

// ---------------------------------------------------------------------------
// Offline provider (always-available fallback)
// ---------------------------------------------------------------------------

pub struct OfflineProvider {
    pub username: String,
}

impl OfflineProvider {
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
        }
    }
}

impl AuthProvider for OfflineProvider {
    async fn authenticate(&self) -> Result<Session, AuthError> {
        Ok(Session::offline(&self.username))
    }
}

// ---------------------------------------------------------------------------
// Microsoft device-code provider
// ---------------------------------------------------------------------------

// Must be the `consumers` tenant: the `XboxLive.signin` scope is only valid there
// (`/common/` or an org tenant returns invalid_scope).
const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";

/// Details the user must act on to complete the device-code flow.
#[derive(Clone, Debug)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
}

/// Callback used to surface the device code (e.g. to the UI) without this layer
/// knowing anything about the UI.
pub type DevicePrompt = Arc<dyn Fn(&DeviceCode) + Send + Sync>;

/// Microsoft OAuth via the device-code flow. Requires an Azure-registered public
/// `client_id` with the `XboxLive.signin` scope.
pub struct MicrosoftProvider {
    pub client_id: String,
    pub prompt: DevicePrompt,
}

impl MicrosoftProvider {
    pub fn new(client_id: impl Into<String>, prompt: DevicePrompt) -> Self {
        Self {
            client_id: client_id.into(),
            prompt,
        }
    }
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    message: String,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// A completed online login: the Minecraft session + the OAuth refresh token,
/// which the launcher persists so the user stays logged in across restarts.
#[derive(Serialize, Clone)]
pub struct OnlineLogin {
    pub session: Session,
    pub refresh_token: String,
}

#[derive(Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Deserialize)]
struct McTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct McProfile {
    id: String,
    name: String,
}

impl AuthProvider for MicrosoftProvider {
    async fn authenticate(&self) -> Result<Session, AuthError> {
        Ok(self.login().await?.session)
    }
}

impl MicrosoftProvider {
    /// Full device-code login → session + refresh token (for stay-logged-in).
    pub async fn login(&self) -> Result<OnlineLogin, AuthError> {
        let client = http_client()?;
        let token = self.obtain_ms_token(&client).await?;
        finish_login(&client, token, false).await
    }

    /// Device-code request + poll → Microsoft OAuth token response (access +
    /// refresh).
    async fn obtain_ms_token(&self, client: &reqwest::Client) -> Result<TokenResponse, AuthError> {
        let resp = client
            .post(DEVICE_CODE_URL)
            .form(&[("client_id", self.client_id.as_str()), ("scope", SCOPE)])
            .send()
            .await
            .map_err(|e| AuthError::DeviceCodeRequestFailed(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AuthError::DeviceCodeRequestFailed(e.to_string()))?;
        // On failure Azure returns a JSON error (e.g. AADSTS...) — surface it
        // instead of a generic "error decoding response body".
        let device: DeviceCodeResponse = serde_json::from_str(&text).map_err(|_| {
            let snippet: String = text.chars().take(400).collect();
            let cid: String = self.client_id.chars().take(8).collect();
            AuthError::DeviceCodeRequestFailed(format!(
                "device code {status} (client {cid}…): {snippet}"
            ))
        })?;

        (self.prompt)(&DeviceCode {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            message: device.message.clone(),
        });

        let mut interval = device.interval.max(1);
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;

            let resp: TokenResponse = client
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", self.client_id.as_str()),
                    ("device_code", device.device_code.as_str()),
                ])
                .send()
                .await
                .map_err(|e| AuthError::Network(e.to_string()))?
                .json()
                .await
                .map_err(|e| AuthError::Network(e.to_string()))?;

            if resp.access_token.is_some() {
                return Ok(resp);
            }
            match resp.error.as_deref() {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    interval += 5;
                    continue;
                }
                Some("authorization_declined") | Some("access_denied") => {
                    return Err(AuthError::AuthorizationDeclined)
                }
                Some("expired_token") | None => return Err(AuthError::AuthorizationTimedOut),
                Some(other) => return Err(AuthError::Network(other.to_string())),
            }
        }
    }
}

/// Azure-token Xbox Live auth (`RpsTicket = d=<token>`).
async fn xbox_live(client: &reqwest::Client, ms_token: &str) -> Result<String, AuthError> {
    xbox_live_rps(client, &format!("d={ms_token}")).await
}

/// Returns the Xbox Live token; the userhash is taken later from the XSTS step.
/// `rps_ticket` is the full RpsTicket (Azure flow uses `d=<token>`, the legacy
/// live.com flow uses the access token directly).
async fn xbox_live_rps(client: &reqwest::Client, rps_ticket: &str) -> Result<String, AuthError> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": rps_ticket
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp: XboxResponse = client
        .post(XBL_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::XboxLiveFailed(e.to_string()))?
        .error_for_status()
        .map_err(|e| AuthError::XboxLiveFailed(e.to_string()))?
        .json()
        .await
        .map_err(|e| AuthError::XboxLiveFailed(e.to_string()))?;
    Ok(resp.token)
}

/// Returns the XSTS (token, userhash). The userhash MUST come from the XSTS
/// response and be paired with the XSTS token for `login_with_xbox`.
async fn xsts(client: &reqwest::Client, xbl_token: &str) -> Result<(String, String), AuthError> {
    let body = serde_json::json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let resp = client
        .post(XSTS_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| AuthError::XstsFailed(e.to_string()))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AuthError::XstsFailed(text));
    }
    let parsed: XboxResponse = resp
        .json()
        .await
        .map_err(|e| AuthError::XstsFailed(e.to_string()))?;
    let uhs = parsed
        .display_claims
        .xui
        .first()
        .map(|x| x.uhs.clone())
        .ok_or_else(|| AuthError::XstsFailed("missing uhs".into()))?;
    Ok((parsed.token, uhs))
}

async fn minecraft_token(
    client: &reqwest::Client,
    uhs: &str,
    xsts_token: &str,
) -> Result<String, AuthError> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });
    let resp = client
        .post(MC_LOGIN_URL)
        .json(&body)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        // Surface the server's actual reason instead of a bare status code.
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(300).collect();
        return Err(AuthError::Network(format!("login_with_xbox {status}: {snippet}")));
    }
    let parsed: McTokenResponse = resp
        .json()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;
    Ok(parsed.access_token)
}

async fn minecraft_profile(
    client: &reqwest::Client,
    mc_token: &str,
) -> Result<McProfile, AuthError> {
    let resp = client
        .get(MC_PROFILE_URL)
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|e| AuthError::ProfileFailed(e.to_string()))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AuthError::NoMinecraftAccount);
    }
    resp.error_for_status()
        .map_err(|e| AuthError::ProfileFailed(e.to_string()))?
        .json()
        .await
        .map_err(|e| AuthError::ProfileFailed(e.to_string()))
}

/// Formats a 32-char hex Minecraft id as a dashed UUID.
fn dash_uuid(id: &str) -> String {
    if id.len() != 32 {
        return id.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &id[0..8],
        &id[8..12],
        &id[12..16],
        &id[16..20],
        &id[20..32]
    )
}

// ============================================================================
// Legacy live.com OAuth (bypasses the Azure Minecraft-API approval gate)
//
// Uses Microsoft's first-party Minecraft client id, which is pre-approved for
// the Minecraft API (predates the approval requirement). This is the flow used
// by msmc / minecraft-launcher-lib. The caller opens AUTH_URL in a webview and
// captures the `code` from the redirect to `oauth20_desktop.srf`.
// ============================================================================

const LIVE_CLIENT_ID: &str = "00000000402b5328";
const LIVE_REDIRECT: &str = "https://login.live.com/oauth20_desktop.srf";
const LIVE_REDIRECT_ENC: &str = "https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf";
const LIVE_SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
const LIVE_SCOPE_ENC: &str = "service%3A%3Auser.auth.xboxlive.com%3A%3AMBI_SSL";
const LIVE_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";

/// The URL to open in a webview for the legacy Microsoft login.
/// `prompt=select_account` forces the account chooser so multiple different
/// accounts can be added (otherwise the cached live.com session auto-logs-in the
/// previous one).
pub fn legacy_auth_url() -> String {
    format!(
        "https://login.live.com/oauth20_authorize.srf?client_id={LIVE_CLIENT_ID}\
         &response_type=code&scope={LIVE_SCOPE_ENC}&redirect_uri={LIVE_REDIRECT_ENC}\
         &prompt=select_account"
    )
}

/// The redirect URL prefix the webview must watch for to capture the code.
pub fn legacy_redirect_prefix() -> &'static str {
    LIVE_REDIRECT
}

/// Exchanges the captured authorization `code` for a full Minecraft session +
/// refresh token (first login).
pub async fn login_legacy(code: &str) -> Result<OnlineLogin, AuthError> {
    let client = http_client()?;
    let token: TokenResponse = client
        .post(LIVE_TOKEN_URL)
        .form(&[
            ("client_id", LIVE_CLIENT_ID),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", LIVE_REDIRECT),
            ("scope", LIVE_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;
    finish_login(&client, token, true).await
}

/// Refreshes a stored login so the user stays signed in across restarts. Returns
/// a fresh session + the (possibly rotated) refresh token to persist.
pub async fn refresh_legacy(refresh_token: &str) -> Result<OnlineLogin, AuthError> {
    let client = http_client()?;
    let token: TokenResponse = client
        .post(LIVE_TOKEN_URL)
        .form(&[
            ("client_id", LIVE_CLIENT_ID),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("redirect_uri", LIVE_REDIRECT),
            ("scope", LIVE_SCOPE),
        ])
        .send()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;
    finish_login(&client, token, true).await
}

fn http_client() -> Result<reqwest::Client, AuthError> {
    reqwest::Client::builder()
        .user_agent("celaris-launcher/0.1")
        .build()
        .map_err(|e| AuthError::Network(e.to_string()))
}

/// Refreshes an Azure (device-code) login using the stored refresh token — keeps
/// the user signed in across restarts without re-doing the device-code dance.
pub async fn refresh_azure(client_id: &str, refresh_token: &str) -> Result<OnlineLogin, AuthError> {
    let client = http_client()?;
    let token: TokenResponse = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;
    finish_login(&client, token, false).await
}

/// Shared tail: MS token → XBL → XSTS → Minecraft token → profile.
/// `legacy` = live.com flow (RpsTicket is the token directly); otherwise the
/// Azure flow (RpsTicket = `d=<token>`).
async fn finish_login(client: &reqwest::Client, token: TokenResponse, legacy: bool) -> Result<OnlineLogin, AuthError> {
    let ms_token = token
        .access_token
        .ok_or_else(|| AuthError::Network("kein access_token erhalten".into()))?;
    let refresh = token.refresh_token.unwrap_or_default();

    let xbl_token = if legacy {
        xbox_live_rps(client, &ms_token).await?
    } else {
        xbox_live(client, &ms_token).await?
    };
    let (xsts_token, uhs) = xsts(client, &xbl_token).await?;
    let mc_token = minecraft_token(client, &uhs, &xsts_token).await?;
    let profile = minecraft_profile(client, &mc_token).await?;

    Ok(OnlineLogin {
        session: Session {
            username: profile.name,
            uuid: dash_uuid(&profile.id),
            access_token: mc_token,
            user_type: "msa".to_string(),
        },
        refresh_token: refresh,
    })
}
