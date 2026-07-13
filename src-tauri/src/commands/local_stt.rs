// noFriction Meetings - Local (offline) speech-to-text commands
// Whisper model management: list catalog, report install status, download.

use crate::transcription::local_whisper;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// The supported model catalog (GGML files from the whisper.cpp project).
/// name → (approx download size MB, description)
const MODEL_CATALOG: &[(&str, u64, &str)] = &[
    ("tiny.en", 75, "Fastest, lowest accuracy — old hardware"),
    ("base.en", 142, "Fast with solid accuracy — recommended default"),
    ("small.en", 466, "Better accuracy, ~2-3x slower than base"),
    ("medium.en", 1500, "High accuracy, needs a capable machine"),
    ("large-v3-turbo", 1600, "Best accuracy, multilingual"),
];

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

/// Download a Whisper model (one-time; ~75MB-1.6GB depending on model).
/// Progress is emitted as `whisper_download_progress` events. After this
/// completes, transcription runs fully offline.
#[tauri::command(rename_all = "camelCase")]
pub async fn download_whisper_model(app: AppHandle, model: String) -> Result<(), String> {
    if !MODEL_CATALOG.iter().any(|(name, _, _)| *name == model) {
        return Err(format!("Unknown model: {}", model));
    }
    let models_dir =
        local_whisper::models_dir().ok_or("Local transcription not configured")?;
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;

    let final_path = models_dir.join(format!("ggml-{}.bin", model));
    if final_path.exists() {
        return Ok(()); // already installed
    }
    let part_path = models_dir.join(format!("ggml-{}.bin.part", model));

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        model
    );
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
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        let msg = format!("Download failed: HTTP {}", resp.status());
        emit_progress(0, None, true, Some(msg.clone()));
        return Err(msg);
    }

    let total = resp.content_length();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();

    let mut file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| format!("Cannot create file: {}", e))?;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Download interrupted: {}", e);
                emit_progress(downloaded, total, true, Some(msg.clone()));
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(msg);
            }
        };
        if let Err(e) = file.write_all(&chunk).await {
            let msg = format!("Write failed: {}", e);
            emit_progress(downloaded, total, true, Some(msg.clone()));
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(msg);
        }
        downloaded += chunk.len() as u64;

        // Throttle progress events to ~4/sec
        if last_emit.elapsed().as_millis() > 250 {
            emit_progress(downloaded, total, false, None);
            last_emit = std::time::Instant::now();
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    tokio::fs::rename(&part_path, &final_path)
        .await
        .map_err(|e| format!("Failed to finalize model file: {}", e))?;

    emit_progress(downloaded, total, true, None);
    log::info!(
        "✅ Whisper model {} installed ({:.0} MB)",
        model,
        downloaded as f64 / 1_048_576.0
    );
    Ok(())
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
