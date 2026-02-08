// noFriction Meetings - Transcript Clustering
// Group transcripts into logical segments based on time gaps and topic similarity

use crate::database::{DatabaseManager, Transcript};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// A cluster of transcripts representing a logical segment of a meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptCluster {
    pub id: String,
    pub meeting_id: String,
    pub name: Option<String>,
    pub transcript_ids: Vec<i64>,
    pub start_time: String,
    pub end_time: String,
    pub confidence: f64,
    pub auto_generated: bool,
}

/// Clustering configuration
#[derive(Debug, Clone)]
pub struct ClusteringConfig {
    /// Time gap in seconds to consider a new cluster
    pub gap_threshold_seconds: i64,
    /// Minimum transcripts to form a cluster
    pub min_cluster_size: usize,
    /// Whether to use topic similarity (requires AI)
    pub use_topic_similarity: bool,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            gap_threshold_seconds: 120, // 2 minutes
            min_cluster_size: 3,
            use_topic_similarity: false,
        }
    }
}

/// Transcript Clustering Engine
pub struct ClusteringEngine {
    config: ClusteringConfig,
}

impl ClusteringEngine {
    pub fn new(config: ClusteringConfig) -> Self {
        Self { config }
    }

    /// Cluster transcripts by time gaps
    pub fn cluster_by_time(&self, transcripts: &[Transcript]) -> Vec<Vec<usize>> {
        if transcripts.is_empty() {
            return vec![];
        }

        let mut clusters: Vec<Vec<usize>> = vec![vec![0]];

        for i in 1..transcripts.len() {
            let prev_ts = transcripts[i - 1].timestamp;
            let curr_ts = transcripts[i].timestamp;
            let gap = (curr_ts - prev_ts).num_seconds();

            if gap > self.config.gap_threshold_seconds {
                // Start a new cluster if current has enough items
                if clusters.last().map(|c| c.len()).unwrap_or(0) >= self.config.min_cluster_size {
                    clusters.push(vec![i]);
                } else {
                    // Merge small cluster into this one
                    clusters.last_mut().unwrap().push(i);
                }
            } else {
                // Add to current cluster
                clusters.last_mut().unwrap().push(i);
            }
        }

        // Filter out clusters that are too small
        clusters
            .into_iter()
            .filter(|c| c.len() >= self.config.min_cluster_size)
            .collect()
    }

    /// Cluster transcripts by speaker changes
    pub fn cluster_by_speaker(&self, transcripts: &[Transcript]) -> Vec<Vec<usize>> {
        if transcripts.is_empty() {
            return vec![];
        }

        let mut clusters: Vec<Vec<usize>> = vec![vec![0]];
        let mut current_speaker = transcripts[0].speaker.clone();

        for i in 1..transcripts.len() {
            let speaker = transcripts[i].speaker.clone();

            if speaker != current_speaker {
                // Speaker changed, consider new cluster
                clusters.push(vec![i]);
                current_speaker = speaker;
            } else {
                clusters.last_mut().unwrap().push(i);
            }
        }

        clusters
    }

    /// Create TranscriptCluster objects from clustered indices
    pub fn create_clusters(
        &self,
        meeting_id: &str,
        transcripts: &[Transcript],
        clustered_indices: &[Vec<usize>],
    ) -> Vec<TranscriptCluster> {
        clustered_indices
            .iter()
            .enumerate()
            .map(|(cluster_num, indices)| {
                let first_idx = indices.first().copied().unwrap_or(0);
                let last_idx = indices.last().copied().unwrap_or(0);

                let transcript_ids: Vec<i64> = indices
                    .iter()
                    .filter_map(|&i| transcripts.get(i).map(|t| t.id))
                    .collect();

                TranscriptCluster {
                    id: Uuid::new_v4().to_string(),
                    meeting_id: meeting_id.to_string(),
                    name: Some(format!("Segment {}", cluster_num + 1)),
                    transcript_ids,
                    start_time: transcripts
                        .get(first_idx)
                        .map(|t| t.timestamp.to_rfc3339())
                        .unwrap_or_default(),
                    end_time: transcripts
                        .get(last_idx)
                        .map(|t| t.timestamp.to_rfc3339())
                        .unwrap_or_default(),
                    confidence: 0.8,
                    auto_generated: true,
                }
            })
            .collect()
    }

    /// Cluster a meeting's transcripts and save to database
    pub async fn cluster_meeting(
        &self,
        meeting_id: &str,
        database: &Arc<DatabaseManager>,
    ) -> Result<Vec<TranscriptCluster>, String> {
        // Get all transcripts for the meeting
        let transcripts = database
            .get_transcripts(meeting_id)
            .await
            .map_err(|e| format!("Failed to get transcripts: {}", e))?;

        if transcripts.is_empty() {
            return Ok(vec![]);
        }

        // Cluster by time
        let clustered_indices = self.cluster_by_time(&transcripts);

        // Create cluster objects
        let clusters = self.create_clusters(meeting_id, &transcripts, &clustered_indices);

        // Save clusters to database
        for cluster in &clusters {
            let transcript_ids_json =
                serde_json::to_string(&cluster.transcript_ids).unwrap_or_default();

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO transcript_clusters 
                (id, meeting_id, cluster_name, start_time, end_time, transcript_ids, auto_generated, confidence)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&cluster.id)
            .bind(&cluster.meeting_id)
            .bind(&cluster.name)
            .bind(&cluster.start_time)
            .bind(&cluster.end_time)
            .bind(&transcript_ids_json)
            .bind(cluster.auto_generated)
            .bind(cluster.confidence)
            .execute(&*database.get_pool())
            .await
            .map_err(|e| format!("Failed to save cluster: {}", e))?;
        }

        log::info!(
            "Created {} transcript clusters for meeting {}",
            clusters.len(),
            meeting_id
        );

        Ok(clusters)
    }

    /// Merge two clusters
    pub fn merge_clusters(
        clusters: &mut Vec<TranscriptCluster>,
        cluster_a_idx: usize,
        cluster_b_idx: usize,
    ) -> Result<(), String> {
        if cluster_a_idx >= clusters.len() || cluster_b_idx >= clusters.len() {
            return Err("Invalid cluster index".to_string());
        }

        if cluster_a_idx == cluster_b_idx {
            return Err("Cannot merge cluster with itself".to_string());
        }

        // Remove cluster B and merge into A
        let cluster_b = clusters.remove(cluster_b_idx);
        let cluster_a = &mut clusters[if cluster_b_idx < cluster_a_idx {
            cluster_a_idx - 1
        } else {
            cluster_a_idx
        }];

        cluster_a.transcript_ids.extend(cluster_b.transcript_ids);
        cluster_a.transcript_ids.sort();

        // Update end time if B was later
        if cluster_b.end_time > cluster_a.end_time {
            cluster_a.end_time = cluster_b.end_time;
        }

        // Update start time if B was earlier
        if cluster_b.start_time < cluster_a.start_time {
            cluster_a.start_time = cluster_b.start_time;
        }

        cluster_a.auto_generated = false; // Mark as user-modified

        Ok(())
    }

    /// Split a cluster at a specific transcript index
    pub fn split_cluster(
        clusters: &mut Vec<TranscriptCluster>,
        cluster_idx: usize,
        at_transcript_idx: usize,
        transcripts: &[Transcript],
    ) -> Result<(), String> {
        if cluster_idx >= clusters.len() {
            return Err("Invalid cluster index".to_string());
        }

        let cluster = &clusters[cluster_idx];
        let split_pos = cluster
            .transcript_ids
            .iter()
            .position(|&id| id == transcripts[at_transcript_idx].id)
            .ok_or("Transcript not in cluster")?;

        if split_pos == 0 || split_pos >= cluster.transcript_ids.len() {
            return Err("Cannot split at this position".to_string());
        }

        // Create new cluster from second half
        let new_ids: Vec<i64> = cluster.transcript_ids[split_pos..].to_vec();
        let first_new_idx = transcripts
            .iter()
            .position(|t| t.id == new_ids[0])
            .unwrap_or(0);

        let new_cluster = TranscriptCluster {
            id: Uuid::new_v4().to_string(),
            meeting_id: cluster.meeting_id.clone(),
            name: Some(format!(
                "{} (split)",
                cluster.name.clone().unwrap_or_default()
            )),
            transcript_ids: new_ids,
            start_time: transcripts
                .get(first_new_idx)
                .map(|t| t.timestamp.to_rfc3339())
                .unwrap_or_default(),
            end_time: cluster.end_time.clone(),
            confidence: 0.7,
            auto_generated: false,
        };

        // Truncate original cluster
        let cluster = &mut clusters[cluster_idx];
        cluster.transcript_ids.truncate(split_pos);
        cluster.auto_generated = false;

        // Update end time
        if let Some(&last_id) = cluster.transcript_ids.last() {
            if let Some(t) = transcripts.iter().find(|t| t.id == last_id) {
                cluster.end_time = t.timestamp.to_rfc3339();
            }
        }

        // Insert new cluster
        clusters.insert(cluster_idx + 1, new_cluster);

        Ok(())
    }

    /// Calculate text similarity using simple Jaccard index
    pub fn text_similarity(text_a: &str, text_b: &str) -> f64 {
        let lower_a = text_a.to_lowercase();
        let lower_b = text_b.to_lowercase();

        let words_a: std::collections::HashSet<&str> = lower_a.split_whitespace().collect();
        let words_b: std::collections::HashSet<&str> = lower_b.split_whitespace().collect();

        if words_a.is_empty() && words_b.is_empty() {
            return 1.0;
        }

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

impl Default for ClusteringEngine {
    fn default() -> Self {
        Self::new(ClusteringConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_similarity() {
        let sim = ClusteringEngine::text_similarity(
            "hello world this is a test",
            "hello world this is another test",
        );
        assert!(sim > 0.5);

        let sim2 = ClusteringEngine::text_similarity("completely different", "nothing alike");
        assert!(sim2 < 0.2);
    }
}
