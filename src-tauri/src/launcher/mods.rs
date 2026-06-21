//! Mod resolution layer (engine-level, no UI).
//!
//! Scans a directory and/or an explicit candidate set, keeps only Fabric-compatible
//! jars (those carrying a `fabric.mod.json`), de-duplicates by mod id and returns a
//! **deterministic load order**. The result feeds into `CelarisLaunchConfig.mods`
//! *before* the launch engine runs — the runner only ever copies the finished list.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A validated Fabric mod: its declared id and the jar it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMod {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Deserialize)]
struct FabricModJson {
    id: String,
}

/// Reads the Fabric mod id from a jar, or `None` if it is not a Fabric mod.
pub fn read_fabric_mod_id(jar: &Path) -> Option<String> {
    let file = std::fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("fabric.mod.json").ok()?;
    let mut contents = String::new();
    entry.read_to_string(&mut contents).ok()?;
    let meta: FabricModJson = serde_json::from_str(&contents).ok()?;
    Some(meta.id)
}

/// Resolves the deterministic mod load order.
///
/// Gathers jars from `scan_dir` (if given) plus the explicit `candidates`,
/// validates each is a Fabric mod, drops duplicate ids (first by sorted path
/// wins) and returns them sorted by mod id ascending — a stable, reproducible
/// order independent of filesystem enumeration order.
pub fn resolve(candidates: &[PathBuf], scan_dir: Option<&Path>) -> Vec<ResolvedMod> {
    let mut jars: Vec<PathBuf> = Vec::new();
    if let Some(dir) = scan_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "jar").unwrap_or(false) {
                    jars.push(path);
                }
            }
        }
    }
    jars.extend(candidates.iter().cloned());
    // Sort paths first so de-duplication is independent of read_dir() ordering.
    jars.sort();
    jars.dedup();

    let mut resolved: Vec<ResolvedMod> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for jar in jars {
        if let Some(id) = read_fabric_mod_id(&jar) {
            if seen_ids.insert(id.clone()) {
                resolved.push(ResolvedMod { id, path: jar });
            }
        }
    }
    resolved.sort_by(|a, b| a.id.cmp(&b.id));
    resolved
}

/// Flattens resolved mods into the ordered jar-path list consumed by the engine.
pub fn load_order(mods: &[ResolvedMod]) -> Vec<PathBuf> {
    mods.iter().map(|m| m.path.clone()).collect()
}
