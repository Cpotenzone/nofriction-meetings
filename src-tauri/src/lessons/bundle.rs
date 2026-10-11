//! The portable `.lesson` zip bundle (docs/LESSONS.md §4). Read/write with the
//! pure-Rust `zip` crate (deflate/stored only). A `manifest.json` carries the
//! schema version, provenance, per-file SHA-256 + byte length, and the content
//! fingerprint. Import verifies every per-file hash, that the zip file-set
//! exactly matches `manifest.files`, rejects `..`/absolute paths, validates
//! fully in a temp dir, then commits in ONE DB transaction (rollback +
//! temp-dir cleanup on failure). Figures are written as loose PNGs on disk.
//!
//! Taking a lesson is 100% offline. The only network path here is
//! `import_from_url`, which is user-initiated and ships with no default URL.

use crate::lessons::schema::{self, Figure, LessonSet, ValidatedSet};
use crate::lessons::SCHEMA_MAJOR;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Sqlite};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::Path;

pub type BundleError = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub schema_major: u32,
    #[serde(default)]
    pub schema_minor: u32,
    pub lesson_set_id: String,
    pub title: String,
    pub provenance: String,
    pub files: Vec<ManifestFile>,
    pub content_fingerprint: String,
    #[serde(default)]
    pub authored_at: String,
    #[serde(default)]
    pub generator: String,
    #[serde(default)]
    pub figures: Vec<Figure>,
}

pub const FORMAT: &str = "nofriction.lesson";

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Reject a zip/relative entry path that escapes the bundle root.
fn safe_entry_path(path: &str) -> Result<(), BundleError> {
    if path.is_empty() {
        return Err("empty entry path".into());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(format!("absolute entry path rejected: {}", path));
    }
    for comp in p.components() {
        use std::path::Component;
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("unsafe entry path rejected: {}", path));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The raw files read out of a bundle (path → bytes), excluding `manifest.json`.
struct RawBundle {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

/// Read a `.lesson` zip into memory (manifest + files), validating entry paths.
fn read_zip(bytes: &[u8]) -> Result<RawBundle, BundleError> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("not a valid .lesson zip: {}", e))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut manifest_raw: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("zip read error: {}", e))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        safe_entry_path(&name)?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| format!("zip entry read error: {}", e))?;
        if name == "manifest.json" {
            manifest_raw = Some(buf);
        } else {
            files.insert(name, buf);
        }
    }
    let manifest_bytes = manifest_raw.ok_or("bundle has no manifest.json")?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("manifest.json malformed: {}", e))?;
    Ok(RawBundle { manifest, files })
}

/// Read an unzipped bundle directory from disk.
fn read_folder(dir: &Path) -> Result<RawBundle, BundleError> {
    let manifest_bytes =
        std::fs::read(dir.join("manifest.json")).map_err(|e| format!("couldn't read manifest.json: {}", e))?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| format!("manifest.json malformed: {}", e))?;
    let mut files = BTreeMap::new();
    for mf in &manifest.files {
        safe_entry_path(&mf.path)?;
        let bytes = std::fs::read(dir.join(&mf.path)).map_err(|e| format!("couldn't read {}: {}", mf.path, e))?;
        files.insert(mf.path.clone(), bytes);
    }
    Ok(RawBundle { manifest, files })
}

/// Verify the manifest against the bundle's files: per-file SHA-256, exact
/// file-set match, schema major, format tag.
fn verify_integrity(raw: &RawBundle) -> Result<(), BundleError> {
    if raw.manifest.format != FORMAT {
        return Err("not a noFriction lesson bundle".into());
    }
    if raw.manifest.schema_major != SCHEMA_MAJOR {
        return Err("This lesson needs a newer version of noFriction.".into());
    }
    // Every manifest file must be present with a matching hash + byte count.
    let mut manifest_set = std::collections::HashSet::new();
    for mf in &raw.manifest.files {
        safe_entry_path(&mf.path)?;
        manifest_set.insert(mf.path.clone());
        let bytes = raw
            .files
            .get(&mf.path)
            .ok_or_else(|| format!("manifest lists a missing file: {}", mf.path))?;
        if bytes.len() as u64 != mf.bytes {
            return Err(format!("file size mismatch for {}", mf.path));
        }
        if sha256_hex(bytes) != mf.sha256 {
            return Err(format!("hash mismatch for {}", mf.path));
        }
    }
    // No extra files beyond the manifest set.
    for name in raw.files.keys() {
        if !manifest_set.contains(name) {
            return Err(format!("bundle has a file not in the manifest: {}", name));
        }
    }
    Ok(())
}

/// Canonical content fingerprint (§4.4): SHA-256 over the canonicalized content
/// (lessonset + lessons serialized with sorted keys, figure bytes folded by
/// their hashes). Independent of file order / whitespace.
fn content_fingerprint(raw: &RawBundle) -> Result<String, BundleError> {
    let mut h = Sha256::new();
    // lessonset.json, canonicalized.
    let lessonset = raw.files.get("lessonset.json").ok_or("bundle has no lessonset.json")?;
    let v: Value = serde_json::from_slice(lessonset).map_err(|e| format!("lessonset.json: {}", e))?;
    h.update(canonical_json(&v).as_bytes());
    // lessons/*.json in sorted path order.
    for (path, bytes) in &raw.files {
        if path.starts_with("lessons/") && path.ends_with(".json") {
            let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("{}: {}", path, e))?;
            h.update(path.as_bytes());
            h.update(canonical_json(&v).as_bytes());
        }
    }
    // Figures folded by their declared hash (sorted by id).
    let mut figs: Vec<&Figure> = raw.manifest.figures.iter().collect();
    figs.sort_by(|a, b| a.id.cmp(&b.id));
    for f in figs {
        h.update(f.id.as_bytes());
        h.update(f.sha256.as_bytes());
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Serialize a JSON value with sorted object keys (recursive) for a stable
/// fingerprint.
fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{:?}:{}", k, canonical_json(&map[*k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

/// Compute the canonical content fingerprint (§4.4) for a set of bundle files
/// and figures, independent of a `RawBundle`. Used by seed authoring and tests
/// to write a manifest whose declared `contentFingerprint` matches what import
/// recomputes (so the NIT fingerprint check passes). `entries` excludes
/// `manifest.json`.
pub fn fingerprint_for(entries: &[(String, Vec<u8>)], figures: &[Figure]) -> Result<String, BundleError> {
    let raw = RawBundle {
        manifest: Manifest {
            format: FORMAT.into(),
            schema_major: SCHEMA_MAJOR,
            schema_minor: crate::lessons::SCHEMA_MINOR,
            lesson_set_id: String::new(),
            title: String::new(),
            provenance: String::new(),
            files: Vec::new(),
            content_fingerprint: String::new(),
            authored_at: String::new(),
            generator: String::new(),
            figures: figures.to_vec(),
        },
        files: entries.iter().cloned().collect(),
    };
    content_fingerprint(&raw)
}

/// A validated, integrity-checked bundle ready to persist to the catalog.
#[derive(Debug)]
pub struct PreparedBundle {
    pub validated: ValidatedSet,
    pub manifest: Manifest,
    /// figureId → PNG bytes (written to disk at commit).
    pub figure_bytes: HashMap<String, Vec<u8>>,
    pub content_fingerprint: String,
}

/// Validate a bundle fully (integrity + fingerprint + schema validation + PNG
/// decode of every figure) without touching the catalog. The core of §4.6.
fn validate_bundle(raw: RawBundle) -> Result<PreparedBundle, BundleError> {
    verify_integrity(&raw)?;
    let fingerprint = content_fingerprint(&raw)?;
    // The recomputed fingerprint is the authoritative identity. If the manifest
    // declares one, it must match — a mismatch means the declared identity does
    // not describe the bundle's actual content, so reject it rather than leave
    // the field as unvalidated data (review NIT). An empty declared fingerprint
    // is tolerated (older/hand-authored bundles): identity is still recomputed.
    if !raw.manifest.content_fingerprint.is_empty()
        && raw.manifest.content_fingerprint != fingerprint
    {
        return Err("bundle content fingerprint does not match its manifest".into());
    }

    // Parse the lesson set.
    let lessonset_raw = raw.files.get("lessonset.json").ok_or("bundle has no lessonset.json")?;
    let set: LessonSet =
        schema::parse_lesson_set(std::str::from_utf8(lessonset_raw).map_err(|_| "lessonset.json not utf-8")?)?;

    // Decode every figure PNG; fold id → bytes.
    let mut figure_bytes = HashMap::new();
    for f in &raw.manifest.figures {
        safe_entry_path(&f.path)?;
        let bytes = raw
            .files
            .get(&f.path)
            .ok_or_else(|| format!("figure file missing: {}", f.path))?;
        // Re-verify the figure's declared sha256 against the actual bytes.
        // `figures[].sha256` feeds the content fingerprint, so a declared hash
        // inconsistent with the bytes could skew the identity even though the
        // files-list hash catches byte tampering. Assert they agree (NIT).
        if sha256_hex(bytes) != f.sha256 {
            return Err(format!("figure {} sha256 does not match its bytes", f.id));
        }
        let img = image::load_from_memory(bytes).map_err(|_| format!("figure {} does not decode", f.id))?;
        if img.width() != f.width || img.height() != f.height {
            return Err(format!("figure {} dimensions mismatch manifest", f.id));
        }
        figure_bytes.insert(f.id.clone(), bytes.clone());
    }

    // Lesson JSON by id.
    let mut lesson_json: HashMap<String, String> = HashMap::new();
    for (path, bytes) in &raw.files {
        if path.starts_with("lessons/") && path.ends_with(".json") {
            let text = std::str::from_utf8(bytes).map_err(|_| format!("{} not utf-8", path))?;
            // id is the file stem, but validate_set keys off the set's lesson order.
            let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("{}: {}", path, e))?;
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                lesson_json.insert(id.to_string(), text.to_string());
            }
        }
    }

    let validated = schema::validate_set(set, raw.manifest.figures.clone(), &lesson_json)?;
    Ok(PreparedBundle {
        validated,
        manifest: raw.manifest,
        figure_bytes,
        content_fingerprint: fingerprint,
    })
}

/// Validate bytes of a `.lesson` zip (no catalog write).
pub fn validate_bytes(bytes: &[u8]) -> Result<PreparedBundle, BundleError> {
    validate_bundle(read_zip(bytes)?)
}

/// Commit a prepared bundle into the catalog in one DB transaction, forcing
/// `provenance` to `provenance_override` (import → "imported", seed → "seed").
/// Writes figure PNGs + lesson JSON to disk under `<app data>/lessons/<id>/`.
/// On any failure the DB transaction rolls back and the temp dir is cleaned up.
pub async fn commit_bundle(
    pool: &Pool<Sqlite>,
    prepared: &PreparedBundle,
    provenance_override: &str,
    source_recording_id: Option<&str>,
) -> Result<String, BundleError> {
    let set_id = prepared.validated.set.id.clone();
    let root = crate::lessons::lessons_root();
    let final_dir = root.join(&set_id);
    // Stage in a temp dir, then rename into place after the DB commit.
    let staging = root.join(format!(".tmp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(staging.join("figures")).map_err(|e| format!("couldn't create staging dir: {}", e))?;
    std::fs::create_dir_all(staging.join("lessons")).map_err(|e| format!("couldn't create staging dir: {}", e))?;

    let write_result = (|| -> Result<(), BundleError> {
        // lessonset.json (re-serialize the validated, cleaned structure).
        let set_json = serde_json::to_vec_pretty(&prepared.validated.set).map_err(|e| e.to_string())?;
        std::fs::write(staging.join("lessonset.json"), &set_json).map_err(|e| e.to_string())?;
        for lesson in &prepared.validated.lessons {
            let bytes = serde_json::to_vec_pretty(lesson).map_err(|e| e.to_string())?;
            std::fs::write(staging.join("lessons").join(format!("{}.json", lesson.id)), &bytes)
                .map_err(|e| e.to_string())?;
        }
        for fig in &prepared.validated.figures {
            if let Some(bytes) = prepared.figure_bytes.get(&fig.id) {
                std::fs::write(staging.join("figures").join(format!("{}.png", fig.id)), bytes)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let dir_path = final_dir.to_string_lossy().to_string();
    let set = &prepared.validated.set;

    let mut tx = pool.begin().await.map_err(|e| format!("db busy: {}", e))?;
    let db_result = async {
        sqlx::query(
            "INSERT OR REPLACE INTO lesson_sets \
             (id, title, subtitle, level, provenance, source_recording_id, transcript_fingerprint, \
              content_fingerprint, schema_major, dir_path, imported_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&set.id)
        .bind(&set.title)
        .bind(&set.subtitle)
        .bind(&set.level)
        .bind(provenance_override)
        .bind(source_recording_id)
        .bind(&set.transcript_fingerprint)
        .bind(&prepared.content_fingerprint)
        .bind(SCHEMA_MAJOR as i64)
        .bind(&dir_path)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("couldn't save the lesson: {}", e))
    }
    .await;

    match db_result {
        Ok(_) => {
            // Move staging → final before committing the DB row so a reader
            // never sees a row without files. If the move fails, roll back.
            let _ = std::fs::remove_dir_all(&final_dir);
            if let Err(e) = std::fs::rename(&staging, &final_dir) {
                let _ = tx.rollback().await;
                let _ = std::fs::remove_dir_all(&staging);
                return Err(format!("couldn't place the lesson files: {}", e));
            }
            tx.commit().await.map_err(|e| format!("couldn't save the lesson: {}", e))?;
            Ok(set_id)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

/// Import a `.lesson` from zip bytes with provenance forced to `imported`.
pub async fn import_bundle_bytes(pool: &Pool<Sqlite>, bytes: &[u8]) -> Result<String, BundleError> {
    let prepared = validate_bytes(bytes)?;
    commit_bundle(pool, &prepared, "imported", None).await
}

/// Import a `.lesson` file the user picked.
pub async fn import_from_file(pool: &Pool<Sqlite>, path: &Path) -> Result<String, BundleError> {
    let bytes = std::fs::read(path).map_err(|e| format!("couldn't read the file: {}", e))?;
    import_bundle_bytes(pool, &bytes).await
}

/// Import an unzipped bundle folder (authoring/testing).
pub async fn import_from_folder(pool: &Pool<Sqlite>, dir: &Path) -> Result<String, BundleError> {
    let prepared = validate_bundle(read_folder(dir)?)?;
    commit_bundle(pool, &prepared, "imported", None).await
}

/// Import a bundle the user downloaded from a URL. This single fetch is the
/// only network activity; it is user-initiated and there is NO default URL
/// shipped. After download the bundle runs fully offline.
pub async fn import_from_url(pool: &Pool<Sqlite>, url: &str) -> Result<String, BundleError> {
    let parsed = url::Url::parse(url).map_err(|_| "that doesn't look like a valid URL".to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("only http(s) URLs can be imported".into());
    }
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("couldn't download the lesson: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("the download failed ({})", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("the download was interrupted: {}", e))?;
    import_bundle_bytes(pool, &bytes).await
}

/// Export a catalog set back to a `.lesson` zip (round-trips the on-disk files
/// and rebuilds the manifest with fresh per-file hashes). Returns the zip bytes.
pub fn export_bundle(dir: &Path) -> Result<Vec<u8>, BundleError> {
    let manifest_bytes =
        std::fs::read(dir.join("manifest.json")).ok();
    // If a manifest exists on disk use its figure list; else reconstruct from
    // the lessonset. For catalog dirs we rebuild the manifest from scratch.
    let set_bytes = std::fs::read(dir.join("lessonset.json")).map_err(|e| format!("no lessonset.json: {}", e))?;
    let set: LessonSet = serde_json::from_slice(&set_bytes).map_err(|e| format!("lessonset.json: {}", e))?;

    // Collect files: lessonset + lessons/*.json + figures/*.png.
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    entries.push(("lessonset.json".into(), set_bytes.clone()));
    let lessons_dir = dir.join("lessons");
    if lessons_dir.is_dir() {
        let mut names: Vec<_> = std::fs::read_dir(&lessons_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".json"))
            .collect();
        names.sort();
        for name in names {
            let bytes = std::fs::read(lessons_dir.join(&name)).map_err(|e| e.to_string())?;
            entries.push((format!("lessons/{}", name), bytes));
        }
    }
    let mut figures: Vec<Figure> = Vec::new();
    let figures_dir = dir.join("figures");
    if figures_dir.is_dir() {
        let mut names: Vec<_> = std::fs::read_dir(&figures_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".png"))
            .collect();
        names.sort();
        for name in names {
            let bytes = std::fs::read(figures_dir.join(&name)).map_err(|e| e.to_string())?;
            let id = name.trim_end_matches(".png").to_string();
            let (w, h) = image::load_from_memory(&bytes)
                .map(|im| (im.width(), im.height()))
                .unwrap_or((0, 0));
            figures.push(Figure {
                id: id.clone(),
                path: format!("figures/{}", name),
                sha256: sha256_hex(&bytes),
                width: w,
                height: h,
                alt: String::new(),
                reviewed_by: String::new(),
            });
            entries.push((format!("figures/{}", name), bytes));
        }
    }

    // Build the manifest with fresh hashes.
    let files: Vec<ManifestFile> = entries
        .iter()
        .map(|(path, bytes)| ManifestFile {
            path: path.clone(),
            sha256: sha256_hex(bytes),
            bytes: bytes.len() as u64,
            kind: if path.ends_with(".png") { Some("image/png".into()) } else { None },
        })
        .collect();
    // Reconstruct the raw bundle only to compute the fingerprint.
    let raw = RawBundle {
        manifest: Manifest {
            format: FORMAT.into(),
            schema_major: SCHEMA_MAJOR,
            schema_minor: crate::lessons::SCHEMA_MINOR,
            lesson_set_id: set.id.clone(),
            title: set.title.clone(),
            provenance: set.provenance.clone(),
            files: files.clone(),
            content_fingerprint: String::new(),
            authored_at: chrono::Utc::now().to_rfc3339(),
            generator: "nofriction-export 1.0".into(),
            figures: figures.clone(),
        },
        files: entries.iter().cloned().collect(),
    };
    let fingerprint = content_fingerprint(&raw).unwrap_or_default();
    let _ = manifest_bytes;

    let manifest = Manifest { content_fingerprint: fingerprint, ..raw.manifest };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;

    write_zip(&manifest_json, &entries)
}

/// Write a `.lesson` zip from a manifest + files (deflate).
pub fn write_zip(manifest_json: &[u8], entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, BundleError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("manifest.json", opts).map_err(|e| e.to_string())?;
        zw.write_all(manifest_json).map_err(|e| e.to_string())?;
        for (path, bytes) in entries {
            zw.start_file(path.clone(), opts).map_err(|e| e.to_string())?;
            zw.write_all(bytes).map_err(|e| e.to_string())?;
        }
        zw.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}
