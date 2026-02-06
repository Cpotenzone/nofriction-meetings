//! Pinecone Client for vector embeddings and semantic search
//!
//! Uses Pinecone's integrated embedding (llama-text-embed-v2) for auto-embedding.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for Pinecone
#[derive(Debug, Clone)]
pub struct PineconeConfig {
    pub api_key: String,
    pub index_host: String,
    pub namespace: Option<String>,
}

/// Vector match result from Pinecone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMatch {
    pub id: String,
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}

/// Metadata for activity vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityMetadata {
    pub timestamp: String,
    pub category: String,
    pub app_name: Option<String>,
    pub focus_area: Option<String>,
    pub summary: String,
}

/// Pinecone client using integrated embeddings
pub struct PineconeClient {
    config: Arc<RwLock<Option<PineconeConfig>>>,
}

impl PineconeClient {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(None)),
        }
    }

    /// Configure the client
    pub fn configure(&self, api_key: String, index_host: String, namespace: Option<String>) {
        *self.config.write() = Some(PineconeConfig {
            api_key,
            index_host,
            namespace,
        });
    }

    /// Check if configured
    pub fn is_configured(&self) -> bool {
        self.config.read().is_some()
    }

    /// Get a clone of the config (for async operations)
    pub fn get_config(&self) -> Option<PineconeConfig> {
        self.config.read().clone()
    }

    /// Upsert text with integrated embedding
    /// Pinecone will automatically embed the text using llama-text-embed-v2
    pub async fn upsert_with_text(
        &self,
        id: &str,
        text: &str,
        metadata: &ActivityMetadata,
    ) -> Result<(), String> {
        let config = self
            .config
            .read()
            .clone()
            .ok_or("Pinecone not configured")?;

        let url = format!(
            "{}/records/namespaces/{}/upsert",
            config.index_host,
            config.namespace.as_deref().unwrap_or("default")
        );

        // Use Pinecone's integrated embedding API
        let request_body = serde_json::json!({
            "records": [{
                "_id": id,
                "text": text,
                "category": metadata.category,
                "app_name": metadata.app_name,
                "focus_area": metadata.focus_area,
                "summary": metadata.summary,
                "timestamp": metadata.timestamp
            }]
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Api-Key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to upsert to Pinecone: {}", e))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(format!("Pinecone upsert failed: {}", error_text));
        }

        log::info!("📌 Vector upserted to Pinecone: {}", id);
        Ok(())
    }

    /// Semantic search using text query (auto-embedded)
    pub async fn search(&self, query: &str, top_k: u32) -> Result<Vec<VectorMatch>, String> {
        let config = self
            .config
            .read()
            .clone()
            .ok_or("Pinecone not configured")?;

        let url = format!(
            "{}/records/namespaces/{}/search",
            config.index_host,
            config.namespace.as_deref().unwrap_or("default")
        );

        let request_body = serde_json::json!({
            "query": {
                "top_k": top_k,
                "inputs": {
                    "text": query
                }
            },
            "fields": ["category", "app_name", "focus_area", "summary", "timestamp"]
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Api-Key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to search Pinecone: {}", e))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(format!("Pinecone search failed: {}", error_text));
        }

        #[derive(Deserialize)]
        struct SearchResponse {
            result: Option<SearchResult>,
        }

        #[derive(Deserialize)]
        struct SearchResult {
            hits: Option<Vec<Hit>>,
        }

        #[derive(Deserialize)]
        struct Hit {
            _id: String,
            _score: Option<f32>,
            fields: Option<serde_json::Value>,
        }

        let search_resp: SearchResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Pinecone response: {}", e))?;

        let matches = search_resp
            .result
            .and_then(|r| r.hits)
            .unwrap_or_default()
            .into_iter()
            .map(|hit| VectorMatch {
                id: hit._id,
                score: hit._score.unwrap_or(0.0),
                metadata: hit.fields,
            })
            .collect();

        Ok(matches)
    }

    /// Delete vectors by ID
    pub async fn delete(&self, ids: &[String]) -> Result<(), String> {
        let config = self
            .config
            .read()
            .clone()
            .ok_or("Pinecone not configured")?;

        let url = format!("{}/vectors/delete", config.index_host);

        let request_body = serde_json::json!({
            "ids": ids,
            "namespace": config.namespace.as_deref().unwrap_or("default")
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Api-Key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to delete from Pinecone: {}", e))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(format!("Pinecone delete failed: {}", error_text));
        }

        Ok(())
    }

    /// Get index stats
    pub async fn describe_index_stats(&self) -> Result<serde_json::Value, String> {
        let config = self
            .config
            .read()
            .clone()
            .ok_or("Pinecone not configured")?;

        let url = format!("{}/describe_index_stats", config.index_host);

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("Api-Key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| format!("Failed to get Pinecone stats: {}", e))?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(format!("Pinecone stats failed: {}", error_text));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse Pinecone stats: {}", e))
    }
}

impl Default for PineconeClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Standalone async functions (avoid RwLock guard issues)
// ============================================

/// Search Pinecone with provided config (no guard held)
pub async fn pinecone_search(
    config: &PineconeConfig,
    query: &str,
    top_k: u32,
) -> Result<Vec<VectorMatch>, String> {
    let url = format!(
        "{}/records/namespaces/{}/search",
        config.index_host,
        config.namespace.as_deref().unwrap_or("default")
    );

    let request_body = serde_json::json!({
        "query": {
            "top_k": top_k,
            "inputs": {
                "text": query
            }
        },
        "fields": ["category", "app_name", "focus_area", "summary", "timestamp"]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Api-Key", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to search Pinecone: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Pinecone search failed: {}", error_text));
    }

    #[derive(serde::Deserialize)]
    struct SearchResponse {
        result: Option<SearchResult>,
    }

    #[derive(serde::Deserialize)]
    struct SearchResult {
        hits: Option<Vec<Hit>>,
    }

    #[derive(serde::Deserialize)]
    struct Hit {
        _id: String,
        _score: Option<f32>,
        fields: Option<serde_json::Value>,
    }

    let search_resp: SearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Pinecone response: {}", e))?;

    let matches = search_resp
        .result
        .and_then(|r| r.hits)
        .unwrap_or_default()
        .into_iter()
        .map(|hit| VectorMatch {
            id: hit._id,
            score: hit._score.unwrap_or(0.0),
            metadata: hit.fields,
        })
        .collect();

    Ok(matches)
}

/// Get Pinecone index stats with provided config (no guard held)
pub async fn pinecone_stats(config: &PineconeConfig) -> Result<serde_json::Value, String> {
    let url = format!("{}/describe_index_stats", config.index_host);

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Api-Key", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| format!("Failed to get Pinecone stats: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Pinecone stats failed: {}", error_text));
    }

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse Pinecone stats: {}", e))
}

/// Upsert to Pinecone with provided config (no guard held)
pub async fn pinecone_upsert(
    config: &PineconeConfig,
    id: &str,
    text: &str,
    metadata: &ActivityMetadata,
) -> Result<(), String> {
    let url = format!(
        "{}/records/namespaces/{}/upsert",
        config.index_host,
        config.namespace.as_deref().unwrap_or("default")
    );

    let request_body = serde_json::json!({
        "records": [{
            "_id": id,
            "text": text,
            "category": metadata.category,
            "app_name": metadata.app_name,
            "focus_area": metadata.focus_area,
            "summary": metadata.summary,
            "timestamp": metadata.timestamp
        }]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Api-Key", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to upsert to Pinecone: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Pinecone upsert failed: {}", error_text));
    }

    log::info!("📌 Vector upserted to Pinecone: {}", id);
    Ok(())
}

/// Generic upsert to Pinecone with provided config (no guard held)
/// Allows passing arbitrary metadata as serde_json::Value
pub async fn pinecone_upsert_generic(
    config: &PineconeConfig,
    id: &str,
    text: &str,
    metadata: &serde_json::Value,
) -> Result<(), String> {
    let url = format!(
        "{}/records/namespaces/{}/upsert",
        config.index_host,
        config.namespace.as_deref().unwrap_or("default")
    );

    // Construct record with text and ID
    let mut record = serde_json::Map::new();
    record.insert("_id".to_string(), serde_json::Value::String(id.to_string()));
    record.insert(
        "text".to_string(),
        serde_json::Value::String(text.to_string()),
    );

    // Merge metadata fields into the record
    if let serde_json::Value::Object(map) = metadata {
        for (k, v) in map {
            record.insert(k.clone(), v.clone());
        }
    }

    let request_body = serde_json::json!({
        "records": [serde_json::Value::Object(record)]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Api-Key", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to upsert to Pinecone: {}", e))?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.unwrap_or_default();
        return Err(format!("Pinecone upsert failed: {}", error_text));
    }

    log::info!("📌 Generic vector upserted to Pinecone: {}", id);
    Ok(())
}
