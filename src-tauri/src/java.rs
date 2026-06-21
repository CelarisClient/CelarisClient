//! Auto-provisions a Java runtime. If the user has no suitable JDK/JRE, we
//! download a Temurin (Adoptium) JRE for the right major version into the
//! launcher's data dir and use that — so players never have to install Java.

use std::io::Cursor;
use std::path::Path;

use tauri::{AppHandle, Emitter, Manager};

/// (os, arch) tokens for the Adoptium API, or None if unsupported.
fn os_arch() -> Option<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "windows" => "windows",
        "macos" => "mac",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some((os, arch))
}

/// Returns a path to a `java` binary for the given major version, downloading a
/// managed JRE if none is already provisioned. Best-effort; errors bubble up so
/// launch can fall back to the system `java`.
pub async fn ensure_java(app: &AppHandle, major: u32) -> Result<String, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("java")
        .join(major.to_string());

    if let Some(p) = find_java(&base) {
        return Ok(p);
    }

    let (os, arch) = os_arch().ok_or("Plattform für Java-Download nicht unterstützt")?;
    let _ = app.emit("launch-log", format!("Kein passendes Java gefunden — lade Java {major} herunter…"));

    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse"
    );
    let client = crate::launcher::download::client().map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Java-Download fehlgeschlagen: {e}"))?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let _ = app.emit("launch-log", "Entpacke Java…".to_string());
    if os == "windows" {
        extract_zip(&bytes, &base)?;
    } else {
        extract_targz(&bytes, &base)?;
    }

    let java = find_java(&base).ok_or("Java-Binary nach dem Entpacken nicht gefunden")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&java) {
            let mut perm = meta.permissions();
            perm.set_mode(perm.mode() | 0o111);
            let _ = std::fs::set_permissions(&java, perm);
        }
    }
    let _ = app.emit("launch-log", "Java bereit.".to_string());
    Ok(java)
}

/// Recursively finds a `bin/java[.exe]` under `dir`.
fn find_java(dir: &Path) -> Option<String> {
    if !dir.exists() {
        return None;
    }
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                // The java binary lives in a "bin" dir — check it directly.
                let cand = p.join(exe);
                if p.file_name().map(|n| n == "bin").unwrap_or(false) && cand.is_file() {
                    return Some(cand.to_string_lossy().to_string());
                }
                stack.push(p);
            }
        }
    }
    None
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = f.enclosed_name() else { continue };
        let out = dest.join(name);
        if f.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn extract_targz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let gz = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(true);
    archive.unpack(dest).map_err(|e| e.to_string())
}

/// Convenience used at launch: returns a working java path for the MC version,
/// preferring an already-detected one, else provisioning a managed JRE.
pub async fn resolve_or_install(app: &AppHandle, detected: &str, major: u32) -> String {
    if detected != "java" && !detected.is_empty() {
        return detected.to_string();
    }
    match ensure_java(app, major).await {
        Ok(p) => p,
        Err(e) => {
            let _ = app.emit("launch-log", format!("Java-Bereitstellung fehlgeschlagen ({e}) — nutze System-Java."));
            "java".to_string()
        }
    }
}
