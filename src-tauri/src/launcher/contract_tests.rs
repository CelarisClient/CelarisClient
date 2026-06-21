//! Launch-pipeline contract tests.
//!
//! Hermetic and deterministic: no network, no real Minecraft. Each test pins the
//! per-stage validation contract to an explicit [`super::error::ErrorCode`], so a
//! regression surfaces as a precise failed assertion rather than a vague log diff.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::error::{ErrorCode, Stage};
use super::resolver::{self, SelectError};
use super::{auth, download, mods, runner};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut p = std::env::temp_dir();
    p.push(format!("celaris-contract-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

const MANIFEST: &str = r#"{
  "versions": [
    { "id": "1.21.11", "url": "https://example/1.21.11.json", "sha1": "aaa", "type": "release" },
    { "id": "1.20.1",  "url": "https://example/1.20.1.json",  "sha1": "bbb", "type": "release" }
  ]
}"#;

const VERSION_JSON: &str = r#"{
  "mainClass": "net.minecraft.client.main.Main",
  "assets": "26",
  "assetIndex": { "id": "26", "url": "https://a/26.json", "sha1": "deadbeef" },
  "downloads": { "client": { "url": "https://c/client.jar", "sha1": "feedface" } }
}"#;

// ---------------------------------------------------------------------------
// resolve stage: version manifest integrity
// ---------------------------------------------------------------------------

#[test]
fn resolve_selects_known_version() {
    let v = resolver::select_version(MANIFEST, "1.21.11").unwrap();
    assert_eq!(v.id, "1.21.11");
    assert_eq!(v.url, "https://example/1.21.11.json");
    assert_eq!(v.sha1, "aaa");
}

#[test]
fn resolve_rejects_unknown_version() {
    let err = resolver::select_version(MANIFEST, "9.9.9").unwrap_err();
    assert_eq!(err, SelectError::NotFound("9.9.9".to_string()));
}

#[test]
fn resolve_rejects_corrupt_manifest() {
    match resolver::select_version("{ not json", "1.21.11") {
        Err(SelectError::Invalid(_)) => {}
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn resolve_parses_valid_version_json() {
    let v = resolver::parse_version_json(VERSION_JSON).unwrap();
    assert_eq!(v.main_class, "net.minecraft.client.main.Main");
    assert_eq!(v.assets, "26");
    assert_eq!(v.asset_index.sha1, "deadbeef");
}

#[test]
fn resolve_rejects_invalid_version_json() {
    assert!(resolver::parse_version_json("{ broken").is_err());
}

// ---------------------------------------------------------------------------
// download stage: SHA1 correctness
// ---------------------------------------------------------------------------

#[test]
fn download_sha1_matches_known_vector() {
    // Canonical: SHA1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
    assert_eq!(
        download::sha1_hex(b"abc"),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
}

#[test]
fn download_sha1_detects_difference() {
    assert_ne!(download::sha1_hex(b"abc"), download::sha1_hex(b"abd"));
}

#[test]
fn download_file_sha1_roundtrip() {
    let dir = temp_dir("sha1");
    let file = dir.join("blob.bin");
    std::fs::write(&file, b"abc").unwrap();
    assert_eq!(
        download::file_sha1(&file).unwrap(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
}

// ---------------------------------------------------------------------------
// inject stage: Fabric + Celaris mod presence
// ---------------------------------------------------------------------------

#[test]
fn inject_fails_when_resolved_mod_absent() {
    let dir = temp_dir("inject-nomod");
    let expected = vec![dir.join("celaris-client.jar")]; // never written
    let err = runner::validate_injection(&dir, &expected, None, &[]).unwrap_err();
    assert_eq!(err.stage, Stage::Inject);
    assert_eq!(err.code, ErrorCode::ModMissing);
}

#[test]
fn inject_ok_when_mods_present_no_fabric() {
    let dir = temp_dir("inject-mods");
    let mod_jar = dir.join("celaris-client.jar");
    std::fs::write(&mod_jar, b"jar").unwrap();
    assert!(runner::validate_injection(&dir, &[mod_jar], None, &[]).is_ok());
}

#[test]
fn inject_fails_when_fabric_api_absent() {
    let dir = temp_dir("inject-noapi");
    let mod_jar = dir.join("celaris-client.jar");
    std::fs::write(&mod_jar, b"jar").unwrap();
    let missing_api = dir.join("fabric-api.jar");
    let err = runner::validate_injection(&dir, &[mod_jar], Some(&missing_api), &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::FabricApiMissing);
}

#[test]
fn inject_fails_when_fabric_loader_lib_absent() {
    let dir = temp_dir("inject-noloader");
    let mod_jar = dir.join("celaris-client.jar");
    std::fs::write(&mod_jar, b"jar").unwrap();
    let api = dir.join("fabric-api.jar");
    std::fs::write(&api, b"jar").unwrap();
    let missing_lib = dir.join("fabric-loader.jar"); // not created
    let err =
        runner::validate_injection(&dir, &[mod_jar], Some(&api), &[missing_lib]).unwrap_err();
    assert_eq!(err.code, ErrorCode::FabricLoaderMissing);
}

#[test]
fn inject_ok_when_everything_present() {
    let dir = temp_dir("inject-ok");
    let mod_jar = dir.join("celaris-client.jar");
    std::fs::write(&mod_jar, b"jar").unwrap();
    let api = dir.join("fabric-api.jar");
    std::fs::write(&api, b"jar").unwrap();
    let lib = dir.join("fabric-loader.jar");
    std::fs::write(&lib, b"jar").unwrap();
    assert!(runner::validate_injection(&dir, &[mod_jar], Some(&api), &[lib]).is_ok());
}

// ---------------------------------------------------------------------------
// launch stage: process startup success
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn launch_succeeds_for_starting_process() {
    let dir = temp_dir("launch-ok");
    // `/bin/true` starts and exits 0 → counts as a successful startup.
    assert!(runner::spawn_validated("/bin/true", &[], &dir, &[]).is_ok());
}

#[cfg(unix)]
#[test]
fn launch_detects_early_failure() {
    let dir = temp_dir("launch-early");
    // `/bin/false` exits non-zero during the startup grace window.
    let err = runner::spawn_validated("/bin/false", &[], &dir, &[]).unwrap_err();
    assert_eq!(err.stage, Stage::Launch);
    assert_eq!(err.code, ErrorCode::ProcessExitedEarly);
}

#[test]
fn launch_detects_spawn_failure() {
    let dir = temp_dir("launch-nospawn");
    let err = runner::spawn_validated("/no/such/celaris-binary-xyz", &[], &dir, &[]).unwrap_err();
    assert_eq!(err.code, ErrorCode::SpawnFailed);
}

// ---------------------------------------------------------------------------
// structured error formatting
// ---------------------------------------------------------------------------

#[test]
fn error_display_is_stage_tagged() {
    let e = super::error::LaunchError::new(Stage::Download, ErrorCode::Sha1Mismatch, "x");
    assert_eq!(e.to_string(), "[Download/Sha1Mismatch] x");
}

// ---------------------------------------------------------------------------
// auth layer: offline session
// ---------------------------------------------------------------------------

#[test]
fn auth_offline_session_is_deterministic() {
    let a = auth::Session::offline("Player");
    let b = auth::Session::offline("Player");
    assert_eq!(a, b);
    assert_eq!(a.username, "Player");
    assert_eq!(a.access_token, "0");
    assert_eq!(a.user_type, "legacy");
    assert_eq!(a.uuid, auth::offline_uuid("Player"));
    // Different names yield different UUIDs.
    assert_ne!(a.uuid, auth::Session::offline("Other").uuid);
}

// ---------------------------------------------------------------------------
// mod resolution layer: validation + deterministic load order
// ---------------------------------------------------------------------------

/// Writes a minimal jar. With `Some(id)` it contains a `fabric.mod.json`
/// (a valid Fabric mod); with `None` it is a plain jar (not a mod).
fn make_jar(dir: &Path, file: &str, mod_id: Option<&str>) -> PathBuf {
    use zip::write::SimpleFileOptions;
    let path = dir.join(file);
    let f = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(f);
    let opts = SimpleFileOptions::default();
    match mod_id {
        Some(id) => {
            zip.start_file("fabric.mod.json", opts).unwrap();
            write!(zip, "{{\"id\":\"{id}\",\"version\":\"1.0.0\"}}").unwrap();
        }
        None => {
            zip.start_file("README.txt", opts).unwrap();
            write!(zip, "not a mod").unwrap();
        }
    }
    zip.finish().unwrap();
    path
}

#[test]
fn mods_detects_and_rejects_by_fabric_metadata() {
    let dir = temp_dir("mods-detect");
    let modjar = make_jar(&dir, "thing.jar", Some("thing"));
    let plain = make_jar(&dir, "plain.jar", None);
    assert_eq!(mods::read_fabric_mod_id(&modjar).as_deref(), Some("thing"));
    assert_eq!(mods::read_fabric_mod_id(&plain), None);
}

#[test]
fn mods_resolve_filters_and_orders_by_id() {
    let dir = temp_dir("mods-order");
    // Filenames intentionally out of id order; non-fabric jar must be dropped.
    make_jar(&dir, "z-first.jar", Some("aaa"));
    make_jar(&dir, "a-first.jar", Some("zzz"));
    make_jar(&dir, "not-a-mod.jar", None);

    let resolved = mods::resolve(&[], Some(&dir));
    let ids: Vec<&str> = resolved.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["aaa", "zzz"]); // deterministic: sorted by id, plain jar excluded
}

#[test]
fn mods_resolve_dedups_by_id() {
    let scan = temp_dir("mods-dedup-scan");
    make_jar(&scan, "dup-a.jar", Some("dup"));
    let extra = temp_dir("mods-dedup-extra");
    let candidate = make_jar(&extra, "dup-b.jar", Some("dup"));

    let resolved = mods::resolve(&[candidate], Some(&scan));
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "dup");
}
