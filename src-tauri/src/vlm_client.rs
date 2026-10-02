//! Vision (screenshot) analysis through the active vision provider.
//!
//! Images are downscaled (long edge ≤ 1568 px, JPEG) before upload so they
//! stay well under every provider's per-image limit, then sent as an
//! OpenAI-style `image_url` data URL or an Anthropic base64 `image` block by
//! `crate::ai::client`. If no vision-capable model is selected the calls
//! return `AI_NO_VISION` and the scheduler pauses.

use crate::ai::{self, Kind, Msg, Opts};
use base64::Engine;
use serde::{Deserialize, Serialize};

const MAX_IMAGE_EDGE: u32 = 1568;
const VISION_MAX_TOKENS: u32 = 800;

/// Activity context extracted from a screenshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityContext {
    /// Primary application being used
    pub app_name: Option<String>,
    /// Window title or document name
    pub window_title: Option<String>,
    /// High-level category (development, communication, research, etc.)
    pub category: String,
    /// What the user appears to be doing
    pub summary: String,
    /// Specific focus area or task
    pub focus_area: Option<String>,
    /// Visible project or file names
    pub visible_files: Vec<String>,
    /// Confidence score 0-1
    pub confidence: f32,
    /// Extracted entities (people, companies, etc.)
    pub entities: Option<serde_json::Value>,
}

/// Load, downscale and JPEG-encode an image; returns (mime, base64).
pub fn encode_image_for_upload(path: &str) -> Result<(String, String), String> {
    let img = image::open(path).map_err(|e| format!("Failed to read image: {}", e))?;
    let img = if img.width().max(img.height()) > MAX_IMAGE_EDGE {
        img.resize(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
    enc.encode_image(&rgb).map_err(|e| format!("Failed to encode image: {}", e))?;
    Ok((
        "image/jpeg".to_string(),
        base64::engine::general_purpose::STANDARD.encode(out.into_inner()),
    ))
}

/// Pull the first JSON object out of a model answer (lenient: models often
/// wrap JSON in prose or code fences; we never use JSON mode).
pub fn parse_activity(response: &str) -> ActivityContext {
    let prefix: String = response.chars().take(200).collect();
    if let (Some(start), Some(end)) = (response.find('{'), response.rfind('}')) {
        if start < end {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response[start..=end]) {
                let s = |k: &str| parsed.get(k).and_then(|v| v.as_str()).map(String::from);
                return ActivityContext {
                    app_name: s("app_name"),
                    window_title: s("window_title"),
                    category: s("category").unwrap_or_else(|| "unknown".into()),
                    summary: s("summary").unwrap_or_else(|| prefix.clone()),
                    focus_area: s("focus_area"),
                    visible_files: parsed
                        .get("visible_files")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    confidence: parsed.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32,
                    entities: parsed.get("entities").cloned(),
                };
            }
        }
    }
    ActivityContext {
        app_name: None,
        window_title: None,
        category: "unknown".to_string(),
        summary: prefix,
        focus_area: None,
        visible_files: vec![],
        confidence: 0.5,
        entities: None,
    }
}

/// A vision provider is selected, configured and allowed (no network call).
pub async fn vlm_is_available() -> bool {
    ai::is_ready(Kind::Vision)
}

/// Is a vision-capable model selected?
pub async fn vlm_has_vision_model() -> Result<bool, String> {
    Ok(crate::ai::config::snapshot().selection(Kind::Vision).is_some())
}

/// Analyze a single frame on the active vision provider.
pub async fn vlm_analyze_frame(image_path: &str, prompt: &str) -> Result<ActivityContext, String> {
    let path = image_path.to_string();
    let (mime, b64) = tokio::task::spawn_blocking(move || encode_image_for_upload(&path))
        .await
        .map_err(|e| format!("Image task failed: {}", e))??;
    let msgs = vec![Msg::user_with_image(prompt.to_string(), &mime, b64)];
    let answer = ai::complete_vision(msgs, Opts { max_tokens: VISION_MAX_TOKENS, temperature: Some(0.1) })
        .await
        .map_err(String::from)?;
    Ok(parse_activity(&answer))
}

/// Analyze multiple frames (sequentially)
pub async fn vlm_analyze_frames_batch(
    frames: Vec<(String, String)>, // (path, prompt) pairs
) -> Vec<Result<ActivityContext, String>> {
    let mut results = Vec::new();
    for (path, prompt) in frames {
        results.push(vlm_analyze_frame(&path, &prompt).await);
    }
    results
}

/// Short text-only completion on the active text provider (classification).
pub async fn vlm_chat(prompt: &str) -> Result<String, String> {
    crate::ai_client::AIClient::new()
        .complete_with(None, prompt, 500, 0.3)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapped_json() {
        let a = parse_activity("Sure!\n```json\n{\"app_name\":\"Xcode\",\"category\":\"development\",\"summary\":\"coding\",\"confidence\":0.9}\n```");
        assert_eq!(a.app_name.as_deref(), Some("Xcode"));
        assert_eq!(a.category, "development");
        assert!((a.confidence - 0.9).abs() < 1e-6);
        let b = parse_activity("no json here");
        assert_eq!(b.category, "unknown");
    }

    #[test]
    fn downscales_large_images() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.png");
        image::RgbImage::new(3000, 2000).save(&p).unwrap();
        let (mime, b64) = encode_image_for_upload(p.to_str().unwrap()).unwrap();
        assert_eq!(mime, "image/jpeg");
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!(img.width(), 1568);
    }
}
