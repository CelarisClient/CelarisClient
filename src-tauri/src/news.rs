//! Newspaper feed: official Minecraft patch notes (Mojang launcher content) plus
//! remotely managed Celaris announcements / changelog from `<CONTENT_BASE>/news.json`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::launcher::download;
use crate::CONTENT_BASE;

const MC_PATCH_NOTES: &str = "https://launchercontent.mojang.com/v2/javaPatchNotes.json";
const MC_BASE: &str = "https://launchercontent.mojang.com";

#[derive(Serialize)]
pub struct NewsItem {
    pub source: String, // "celaris" | "minecraft"
    pub title: String,
    pub date: String,
    pub tag: String,
    pub body: String, // short snippet for the grid
    pub full: String, // full text for the detail view
    pub image: Option<String>,
}

// --- Mojang patch notes ---
#[derive(Deserialize)]
struct McPatchNotes {
    entries: Vec<McEntry>,
}

#[derive(Deserialize)]
struct McEntry {
    title: String,
    #[serde(default)]
    version: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    image: Option<McImage>,
    /// Summary shown in the grid.
    #[serde(rename = "shortText", default)]
    short_text: String,
    /// Path to the full patch-notes article (separate JSON).
    #[serde(rename = "contentPath", default)]
    content_path: String,
}

#[derive(Deserialize)]
struct McImage {
    url: String,
}

/// The per-article document referenced by `contentPath`.
#[derive(Deserialize)]
struct McContent {
    #[serde(default)]
    body: String,
}

// --- Celaris announcements (remotely managed) ---
#[derive(Deserialize)]
struct CelarisNews {
    title: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    image: Option<String>,
}

fn strip_html(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = decode_entities(&out);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decodes the handful of HTML entities Mojang's patch notes actually use.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
}

/// Date `days_ago` days before today as `YYYY-MM-DD` (UTC). Used to keep Mojang
/// changelogs for exactly N days. No external date crate — Howard Hinnant's
/// days→civil algorithm. ISO-8601 dates compare correctly as plain strings, so
/// callers just check `entry.date >= cutoff_date(30)`.
fn cutoff_date(days_ago: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = (now - days_ago * 86_400) / 86_400 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

#[tauri::command]
pub async fn fetch_news() -> Result<Vec<NewsItem>, String> {
    let client = download::client().map_err(|e| e.to_string())?;
    let mut items = Vec::new();

    // Celaris announcements first (best-effort — empty if the host is unset/down).
    if let Ok(resp) = client
        .get(format!("{CONTENT_BASE}/news.json"))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        if let Ok(list) = resp.json::<Vec<CelarisNews>>().await {
            for n in list {
                items.push(NewsItem {
                    source: "celaris".into(),
                    title: n.title,
                    date: n.date,
                    tag: n.tag.unwrap_or_else(|| "Celaris".into()),
                    body: truncate(&n.body, 220),
                    full: n.body,
                    image: n.image,
                });
            }
        }
    }

    // Official Minecraft patch notes (best-effort).
    if let Ok(resp) = client
        .get(MC_PATCH_NOTES)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        if let Ok(notes) = resp.json::<McPatchNotes>().await {
            // Auto-show every Mojang changelog from the last 30 days (newest first),
            // and drop them automatically once older. Capped so we don't fetch an
            // unbounded number of article bodies.
            let cutoff = cutoff_date(30);
            let mut entries: Vec<McEntry> = notes
                .entries
                .into_iter()
                .filter(|e| e.date.as_str() >= cutoff.as_str())
                .collect();
            // The feed isn't date-ordered (snapshots, then RCs/pre-releases) →
            // sort newest first so the latest version leads.
            entries.sort_by(|a, b| b.date.cmp(&a.date));
            entries.truncate(15);

            // Fetch each article's full body (contentPath) concurrently.
            let bodies = futures_util::future::join_all(entries.iter().map(|e| {
                let client = client.clone();
                let path = e.content_path.clone();
                async move {
                    if path.is_empty() {
                        return String::new();
                    }
                    match client
                        .get(format!("{MC_BASE}/{path}"))
                        .timeout(Duration::from_secs(8))
                        .send()
                        .await
                        .and_then(|r| r.error_for_status())
                    {
                        Ok(r) => match r.json::<McContent>().await {
                            Ok(c) => strip_html(&c.body),
                            Err(_) => String::new(),
                        },
                        Err(_) => String::new(),
                    }
                }
            }))
            .await;

            for (e, full) in entries.into_iter().zip(bodies) {
                let image = e.image.map(|i| {
                    if i.url.starts_with("http") {
                        i.url
                    } else {
                        format!("{MC_BASE}{}", i.url)
                    }
                });
                let short_clean = strip_html(&e.short_text);
                let snippet = if short_clean.is_empty() {
                    truncate(&full, 340)
                } else {
                    truncate(&short_clean, 340)
                };
                let full = if full.is_empty() { short_clean } else { full };
                items.push(NewsItem {
                    source: "minecraft".into(),
                    title: e.title,
                    date: e.date,
                    tag: if e.kind.is_empty() { e.version } else { e.kind },
                    body: snippet,
                    full,
                    image,
                });
            }
        }
    }

    Ok(items)
}
