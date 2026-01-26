// noFriction Meetings - Ingest Client
// Uploads frames and transcripts to Intelligence Pipeline

use reqwest::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct IngestClient {
    client: Client,
    base_url: String,
    bearer_token: String,
}

#[derive(Debug, Serialize)]
pub struct SessionStartRequest {
    pub session_id: Option<Uuid>,
    pub started_at: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SessionEndRequest {
    pub session_id: Uuid,
    pub ended_at: String,
}

#[derive(Debug, Serialize)]
pub struct TranscriptSegment {
    pub start_at: String,
    pub end_at: String,
    pub text: String,
    pub speaker: Option<String>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TranscriptIngestRequest {
    pub session_id: Uuid,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Deserialize)]
pub struct FrameIngestResponse {
    pub frame_id: Uuid,
    pub sha256: String,
    pub object_path: Option<String>,
    pub duplicate: bool,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptIngestResponse {
    pub session_id: Uuid,
    pub segment_ids: Vec<Uuid>,
    pub count: usize,
}

impl IngestClient {
    pub fn new(base_url: String, bearer_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            bearer_token,
        }
    }

    /// Start a new session
    pub async fn start_session(
        &self,
        session_id: Option<Uuid>,
        started_at: String,
        metadata: serde_json::Value,
    ) -> Result<Uuid, Box<dyn std::error::Error>> {
        let url = format!("{}/v1/ingest/session/start", self.base_url);

        let request = SessionStartRequest {
            session_id,
            started_at,
            metadata,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.bearer_token))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(format!("Session start failed: {} - {}", status, body).into());
        }

        let result: serde_json::Value = response.json().await?;
        let session_id = result["session_id"]
            .as_str()
            .ok_or("Missing session_id in response")?;

        Ok(Uuid::parse_str(session_id)?)
    }

    /// End a session
    pub async fn end_session(
        &self,
        session_id: Uuid,
        ended_at: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/v1/ingest/session/end", self.base_url);

        let request = SessionEndRequest {
            session_id,
            ended_at,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.bearer_token))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(format!("Session end failed: {} - {}", status, body).into());
        }

        Ok(())
    }

    /// Upload a frame (image)
    pub async fn upload_frame(
        &self,
        session_id: Uuid,
        captured_at: String,
        image_path: &Path,
        sha256: Option<String>,
    ) -> Result<FrameIngestResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/v1/ingest/frame", self.base_url);

        // Read image file
        let file_data = fs::read(image_path).await?;
        let file_name = image_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("frame.jpg");

        // Build multipart form
        let mut form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .text("captured_at", captured_at)
            .part(
                "file",
                multipart::Part::bytes(file_data)
                    .file_name(file_name.to_string())
                    .mime_str("image/jpeg")?,
            );

        if let Some(hash) = sha256 {
            form = form.text("sha256", hash);
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.bearer_token))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(format!("Frame upload failed: {} - {}", status, body).into());
        }

        Ok(response.json::<FrameIngestResponse>().await?)
    }

    /// Upload transcript segments
    pub async fn upload_transcript(
        &self,
        session_id: Uuid,
        segments: Vec<TranscriptSegment>,
    ) -> Result<TranscriptIngestResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/v1/ingest/transcript", self.base_url);

        let request = TranscriptIngestRequest {
            session_id,
            segments,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.bearer_token))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(format!("Transcript upload failed: {} - {}", status, body).into());
        }

        Ok(response.json::<TranscriptIngestResponse>().await?)
    }

    /// Health check
    pub async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let url = format!("{}/health", self.base_url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        Ok(response.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = IngestClient::new(
            "http://localhost:8000".to_string(),
            "test_token".to_string(),
        );
        assert_eq!(client.base_url, "http://localhost:8000");
    }
}
