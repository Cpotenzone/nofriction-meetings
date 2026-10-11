// noFriction Meetings - Local (offline) speech-to-text commands
// Whisper model management: list catalog, report install status, download.

use crate::transcription::local_whisper;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// The supported model catalog (GGML files from the whisper.cpp project).
/// name → (approx download size MB, description)
const MODEL_CATALOG: &[(&str, u64, &str)] = &[
    ("tiny.en", 75, "Smallest and fastest — fallback for old hardware"),
    ("base.en", 142, "Light with decent accuracy"),
    ("small.en", 466, "Good accuracy, fast on any Apple Silicon Mac"),
    ("large-v3-turbo-q5_0", 547, "Recommended — near large-v3 accuracy at small-model speed"),
    ("medium.en", 1500, "High accuracy, needs a capable machine"),
    ("large-v3-turbo", 1600, "Full-precision turbo, multilingual"),
];

/// The whisper.cpp model repository revision the app downloads from. Pinned
/// (not `main`) so the files can't change under the hashes below.
const MODEL_REPO_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// SHA-256 of every model in `MODEL_CATALOG`, checked after each download.
///
/// Source: the Git LFS pointers of the official whisper.cpp model repository
/// on Hugging Face, `ggerganov/whisper.cpp` at revision
/// 5359861c739e955e79d9a303bcbc70fb988958b1 (each file's `lfs.oid` is its
/// SHA-256):
///   https://huggingface.co/api/models/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1
///   https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1
/// The download response's `X-Linked-Etag` header reports the same values.
/// Cross-check: ggml-tiny.en.bin downloaded from that revision hashed to the
/// SHA-256 below and to SHA-1 c78c86eb1a8faa21b369bcd33207cc90d64ae9df, the
/// value listed in whisper.cpp's models/README.md
/// (https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md).
const MODEL_SHA256: &[(&str, &str)] = &[
    ("tiny.en", "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f"),
    ("base.en", "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"),
    ("small.en", "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d"),
    ("large-v3-turbo-q5_0", "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"),
    ("medium.en", "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356"),
    ("large-v3-turbo", "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
];

/// Shown when a finished download doesn't match its published hash.
pub const DOWNLOAD_DAMAGED: &str = "The download was damaged. Try again.";

fn expected_sha256(model: &str) -> Option<&'static str> {
    MODEL_SHA256.iter().find(|(m, _)| *m == model).map(|(_, h)| *h)
}

fn model_url(model: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/{}/ggml-{}.bin",
        MODEL_REPO_REVISION, model
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct WhisperModelInfo {
    pub name: String,
    pub size_mb: u64,
    pub description: String,
    pub installed: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalSttStatus {
    pub models_dir: Option<String>,
    pub preferred_model: String,
    pub ready: bool,
    pub resolved_model: Option<String>,
    pub models: Vec<WhisperModelInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
    model: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    done: bool,
    error: Option<String>,
}

/// Status of local transcription: which models are installed and whether
/// the provider can start right now.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_local_stt_status() -> Result<LocalSttStatus, String> {
    let models_dir = local_whisper::models_dir();
    let preferred = local_whisper::preferred_model();
    let resolved = local_whisper::resolve_model_path().ok();

    let models = MODEL_CATALOG
        .iter()
        .map(|(name, size_mb, desc)| {
            let installed = models_dir
                .as_ref()
                .map(|d| d.join(format!("ggml-{}.bin", name)).exists())
                .unwrap_or(false);
            WhisperModelInfo {
                name: name.to_string(),
                size_mb: *size_mb,
                description: desc.to_string(),
                installed,
                active: *name == preferred,
            }
        })
        .collect();

    Ok(LocalSttStatus {
        models_dir: models_dir.map(|p| p.to_string_lossy().to_string()),
        preferred_model: preferred,
        ready: resolved.is_some(),
        resolved_model: resolved.and_then(|p| {
            p.file_name().map(|f| f.to_string_lossy().to_string())
        }),
        models,
    })
}

/// Select which installed model local transcription uses.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_local_whisper_model(
    model: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !MODEL_CATALOG.iter().any(|(name, _, _)| *name == model) {
        return Err(format!("Unknown model: {}", model));
    }
    local_whisper::set_preferred_model(&model);
    state
        .settings
        .set_local_whisper_model(&model)
        .await
        .map_err(|e| format!("Failed to save setting: {}", e))?;
    log::info!("Local Whisper model set to: {}", model);
    Ok(())
}

/// Write a download to `part_path` while hashing it, then move it to
/// `final_path` only if its SHA-256 is `expected_sha256`. On any failure the
/// partial file is deleted and `final_path` is never created, so a damaged
/// model can't be found or loaded (the loader only looks at `ggml-*.bin`).
/// Returns the number of bytes written.
async fn save_verified<S, B, E>(
    mut stream: S,
    part_path: &std::path::Path,
    final_path: &std::path::Path,
    expected_sha256: &str,
    mut on_progress: impl FnMut(u64),
) -> Result<u64, String>
where
    S: futures_util::Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(part_path)
        .await
        .map_err(|e| format!("Cannot create file: {}", e))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;

    let written: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download interrupted: {}", e))?;
            let bytes = chunk.as_ref();
            file.write_all(bytes)
                .await
                .map_err(|e| format!("Write failed: {}", e))?;
            hasher.update(bytes);
            downloaded += bytes.len() as u64;
            on_progress(downloaded);
        }
        file.flush().await.map_err(|e| e.to_string())
    }
    .await;
    drop(file);
    if let Err(e) = written {
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(e);
    }

    let actual: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = tokio::fs::remove_file(part_path).await;
        log::warn!(
            "Whisper model download failed its checksum (expected {}, got {}); deleted",
            expected_sha256,
            actual
        );
        return Err(DOWNLOAD_DAMAGED.to_string());
    }

    if let Err(e) = tokio::fs::rename(part_path, final_path).await {
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(format!("Failed to finalize model file: {}", e));
    }
    Ok(downloaded)
}

/// Download a Whisper model (one-time; ~75MB-1.6GB depending on model).
/// Progress is emitted as `whisper_download_progress` events. The file is
/// checked against its published SHA-256 before it is installed. After this
/// completes, transcription runs fully offline.
#[tauri::command(rename_all = "camelCase")]
pub async fn download_whisper_model(app: AppHandle, model: String) -> Result<(), String> {
    if !MODEL_CATALOG.iter().any(|(name, _, _)| *name == model) {
        return Err(format!("Unknown model: {}", model));
    }
    // Every catalog model has a hash (see the tests); never install one without.
    let expected =
        expected_sha256(&model).ok_or_else(|| format!("No checksum for model: {}", model))?;
    let models_dir =
        local_whisper::models_dir().ok_or("Local transcription not configured")?;
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;

    let final_path = models_dir.join(format!("ggml-{}.bin", model));
    if final_path.exists() {
        return Ok(()); // already installed
    }
    let part_path = models_dir.join(format!("ggml-{}.bin.part", model));

    let url = model_url(&model);
    log::info!("⬇️ Downloading Whisper model {} from {}", model, url);

    let emit_progress = |downloaded: u64, total: Option<u64>, done: bool, error: Option<String>| {
        let _ = app.emit(
            "whisper_download_progress",
            DownloadProgress {
                model: model.clone(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                done,
                error,
            },
        );
    };

    let client = reqwest::Client::new();
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("Download failed: {}", e);
            emit_progress(0, None, true, Some(msg.clone()));
            return Err(msg);
        }
    };

    if !resp.status().is_success() {
        let msg = format!("Download failed: HTTP {}", resp.status());
        emit_progress(0, None, true, Some(msg.clone()));
        return Err(msg);
    }

    let total = resp.content_length();
    let mut last_emit = std::time::Instant::now();
    let saved = save_verified(resp.bytes_stream(), &part_path, &final_path, expected, |downloaded| {
        // Throttle progress events to ~4/sec
        if last_emit.elapsed().as_millis() > 250 {
            emit_progress(downloaded, total, false, None);
            last_emit = std::time::Instant::now();
        }
    })
    .await;

    match saved {
        Ok(downloaded) => {
            emit_progress(downloaded, total, true, None);
            log::info!(
                "✅ Whisper model {} installed and verified ({:.0} MB)",
                model,
                downloaded as f64 / 1_048_576.0
            );
            Ok(())
        }
        Err(msg) => {
            emit_progress(0, total, true, Some(msg.clone()));
            Err(msg)
        }
    }
}

/// Delete an installed model to free disk space.
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_whisper_model(model: String) -> Result<(), String> {
    let models_dir =
        local_whisper::models_dir().ok_or("Local transcription not configured")?;
    let path = models_dir.join(format!("ggml-{}.bin", model));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        log::info!("🗑 Deleted Whisper model {}", model);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    const FIXTURE: &[u8] = b"noFriction model fixture\n";

    /// Independently computed: `printf 'noFriction model fixture\n' | shasum -a 256`
    const FIXTURE_SHA256: &str = "15a37efc50c8ee398a52cf8de48ad449f36bdc06853329dbab0c6057934f9b90";

    fn fixture_sha256() -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(FIXTURE).iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// The fixture, split into chunks the way a download arrives.
    fn chunks() -> impl futures_util::Stream<Item = Result<Vec<u8>, String>> + Unpin {
        stream::iter(FIXTURE.chunks(7).map(|c| Ok(c.to_vec())).collect::<Vec<_>>())
    }

    #[test]
    fn fixture_hash_is_the_known_value() {
        assert_eq!(fixture_sha256(), FIXTURE_SHA256);
    }

    #[test]
    fn every_catalog_model_has_a_published_sha256() {
        for (name, _, _) in MODEL_CATALOG {
            let h = expected_sha256(name).unwrap_or_else(|| panic!("no SHA-256 for {}", name));
            assert_eq!(h.len(), 64, "{}", name);
            assert!(
                h.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "{}",
                name
            );
        }
        assert_eq!(MODEL_SHA256.len(), MODEL_CATALOG.len());
    }

    #[test]
    fn downloads_come_from_the_pinned_revision() {
        assert_eq!(
            model_url("tiny.en"),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-tiny.en.bin"
        );
    }

    #[tokio::test]
    async fn matching_download_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("ggml-test.bin.part");
        let fin = dir.path().join("ggml-test.bin");
        let mut seen = 0;
        let n = save_verified(chunks(), &part, &fin, FIXTURE_SHA256, |d| seen = d)
            .await
            .unwrap();
        assert_eq!(n, FIXTURE.len() as u64);
        assert_eq!(seen, FIXTURE.len() as u64);
        assert_eq!(std::fs::read(&fin).unwrap(), FIXTURE);
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn uppercase_expected_hash_still_matches() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("m.part");
        let fin = dir.path().join("m.bin");
        save_verified(chunks(), &part, &fin, &FIXTURE_SHA256.to_uppercase(), |_| {})
            .await
            .unwrap();
        assert!(fin.exists());
    }

    #[tokio::test]
    async fn mismatched_download_is_deleted_and_never_installed() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("ggml-test.bin.part");
        let fin = dir.path().join("ggml-test.bin");
        let wrong = "0".repeat(64);
        let err = save_verified(chunks(), &part, &fin, &wrong, |_| {}).await.unwrap_err();
        assert_eq!(err, "The download was damaged. Try again.");
        assert_eq!(err, DOWNLOAD_DAMAGED);
        assert!(!part.exists(), "partial file must be deleted");
        assert!(!fin.exists(), "a damaged model must never be installed");
    }

    #[tokio::test]
    async fn truncated_download_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("m.part");
        let fin = dir.path().join("m.bin");
        let half = stream::iter(vec![Ok::<_, String>(FIXTURE[..10].to_vec())]);
        let err = save_verified(half, &part, &fin, FIXTURE_SHA256, |_| {}).await.unwrap_err();
        assert_eq!(err, DOWNLOAD_DAMAGED);
        assert!(!part.exists() && !fin.exists());
    }

    #[tokio::test]
    async fn interrupted_download_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("m.part");
        let fin = dir.path().join("m.bin");
        let broken = stream::iter(vec![Ok(FIXTURE[..10].to_vec()), Err("connection reset".to_string())]);
        let err = save_verified(broken, &part, &fin, FIXTURE_SHA256, |_| {}).await.unwrap_err();
        assert!(err.starts_with("Download interrupted"), "{}", err);
        assert!(!part.exists() && !fin.exists());
    }
}
