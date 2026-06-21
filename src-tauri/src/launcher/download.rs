//! HTTP helpers and a checksummed, parallel file downloader.
//!
//! Transport-level errors are returned as [`DownloadError`] so each pipeline
//! stage can map them onto its own [`super::error::ErrorCode`] — in particular
//! distinguishing a SHA1 mismatch from a network failure.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};

use super::{Progress, Reporter};

/// A single file to fetch. `sha1` is verified when present.
#[derive(Clone)]
pub struct DownloadItem {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
}

/// Transport-level failure.
#[derive(Debug, Clone)]
pub enum DownloadError {
    /// Network / HTTP status failure.
    Http(String),
    /// Response body failed to deserialize.
    Parse(String),
    /// Local filesystem failure.
    Io(String),
    /// Checksum verification failed.
    Sha1Mismatch {
        url: String,
        expected: String,
        got: String,
    },
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::Http(m) => write!(f, "http error: {m}"),
            DownloadError::Parse(m) => write!(f, "parse error: {m}"),
            DownloadError::Io(m) => write!(f, "io error: {m}"),
            DownloadError::Sha1Mismatch { url, expected, got } => {
                write!(f, "sha1 mismatch for {url}: got {got}, expected {expected}")
            }
        }
    }
}

/// Builds a shared HTTP client with a sane user agent.
pub fn client() -> Result<reqwest::Client, DownloadError> {
    reqwest::Client::builder()
        .user_agent("celaris-launcher/0.1")
        .build()
        .map_err(|e| DownloadError::Http(e.to_string()))
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String, DownloadError> {
    client
        .get(url)
        .send()
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| DownloadError::Http(format!("GET {url}: {e}")))?
        .text()
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))
}

pub async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, DownloadError> {
    let text = get_text(client, url).await?;
    serde_json::from_str(&text).map_err(|e| DownloadError::Parse(format!("{url}: {e}")))
}

pub async fn get_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, DownloadError> {
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| DownloadError::Http(format!("GET {url}: {e}")))?
        .bytes()
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))?;
    Ok(bytes.to_vec())
}

/// Hex-encoded SHA-1 of a byte slice.
pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Hex-encoded SHA-1 of a file on disk, or `None` if unreadable.
pub fn file_sha1(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(sha1_hex(&bytes))
}

/// Downloads `url` to `dest`, skipping the work if a valid copy already exists.
/// When `sha1` is provided, the downloaded bytes are verified before writing.
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    sha1: Option<&str>,
) -> Result<(), DownloadError> {
    if dest.exists() {
        match sha1 {
            Some(expected) if file_sha1(dest).as_deref() == Some(expected) => return Ok(()),
            None => return Ok(()),
            _ => {} // present but checksum mismatch → re-download
        }
    }

    let bytes = get_bytes(client, url).await?;

    if let Some(expected) = sha1 {
        let got = sha1_hex(&bytes);
        if got != expected {
            return Err(DownloadError::Sha1Mismatch {
                url: url.to_string(),
                expected: expected.to_string(),
                got,
            });
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DownloadError::Io(e.to_string()))?;
    }
    std::fs::write(dest, &bytes).map_err(|e| DownloadError::Io(e.to_string()))
}

/// Downloads many files with bounded concurrency, reporting progress. Fails fast
/// on the first error.
pub async fn download_many(
    client: &reqwest::Client,
    items: Vec<DownloadItem>,
    reporter: &dyn Reporter,
    stage: &str,
) -> Result<(), DownloadError> {
    let total = items.len() as u64;
    if total == 0 {
        return Ok(());
    }
    let done = Arc::new(AtomicU64::new(0));

    let mut stream = stream::iter(items.into_iter().map(|item| {
        let client = client.clone();
        let done = done.clone();
        async move {
            download_file(&client, &item.url, &item.dest, item.sha1.as_deref()).await?;
            Ok::<u64, DownloadError>(done.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }))
    .buffer_unordered(16);

    while let Some(result) = stream.next().await {
        let current = result?;
        reporter.progress(Progress {
            stage: stage.to_string(),
            message: format!("{current}/{total}"),
            current,
            total,
        });
    }
    Ok(())
}
