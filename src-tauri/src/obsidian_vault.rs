// noFriction Meetings - Obsidian Vault Manager
// Read/write Obsidian-compatible markdown files for meeting knowledge management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Represents a Topic — a top-level organizing folder in the vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultTopic {
    pub name: String,
    pub path: String,
    pub meetings: Vec<String>,
    pub note_count: i32,
    pub created_at: String,
    pub tags: Vec<String>,
}

/// Represents a file or directory in the vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFile {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub modified: String,
    pub size: u64,
    pub extension: Option<String>,
}

/// Contents of a vault file with parsed frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFileContent {
    pub path: String,
    pub content: String,
    pub frontmatter: serde_json::Value,
    pub body: String,
}

/// Hierarchical tree node for vault structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<VaultTreeNode>,
}

/// Vault status info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub configured: bool,
    pub path: Option<String>,
    pub valid: bool,
    pub topic_count: i32,
    pub total_files: i32,
}

/// Search result from vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSearchResult {
    pub file_path: String,
    pub file_name: String,
    pub matching_line: String,
    pub line_number: usize,
    pub context: String,
}

/// The Vault Manager — handles all filesystem operations on the Obsidian vault
pub struct VaultManager {
    vault_path: parking_lot::RwLock<Option<PathBuf>>,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            vault_path: parking_lot::RwLock::new(None),
        }
    }

    /// Set the vault path
    pub fn set_vault_path(&self, path: String) {
        self.vault_path.write().replace(PathBuf::from(path));
    }

    /// Get the configured vault path
    pub fn get_vault_path(&self) -> Option<PathBuf> {
        self.vault_path.read().clone()
    }

    /// Get the noFriction root inside the vault
    fn nofriction_root(&self) -> Option<PathBuf> {
        self.get_vault_path().map(|p| p.join("noFriction"))
    }

    /// Check if vault is configured and valid
    pub async fn get_status(&self) -> VaultStatus {
        let vault_path = self.get_vault_path();
        match vault_path {
            None => VaultStatus {
                configured: false,
                path: None,
                valid: false,
                topic_count: 0,
                total_files: 0,
            },
            Some(ref p) => {
                let valid = p.exists() && p.is_dir();
                let mut topic_count = 0;
                let mut total_files = 0;

                if valid {
                    let topics_dir = p.join("noFriction").join("topics");
                    if topics_dir.exists() {
                        if let Ok(mut entries) = fs::read_dir(&topics_dir).await {
                            while let Ok(Some(entry)) = entries.next_entry().await {
                                if entry.path().is_dir() {
                                    topic_count += 1;
                                }
                            }
                        }
                    }
                    total_files = count_files_recursive(p).await.unwrap_or(0);
                }

                VaultStatus {
                    configured: true,
                    path: Some(p.to_string_lossy().to_string()),
                    valid,
                    topic_count,
                    total_files,
                }
            }
        }
    }

    /// Ensure the noFriction folder structure exists
    pub async fn ensure_structure(&self) -> Result<(), String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let topics = root.join("topics");
        let templates = root.join("templates");

        fs::create_dir_all(&topics)
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(&templates)
            .await
            .map_err(|e| e.to_string())?;

        // Create index if it doesn't exist
        let index_path = root.join("_index.md");
        if !index_path.exists() {
            let index_content = format!(
                "---\ntitle: noFriction Vault\ncreated: {}\ntype: index\n---\n\n# noFriction Meeting Vault\n\nThis folder is managed by [noFriction Meetings](https://nofriction.ai).\n\n## Topics\n\nBrowse the `topics/` folder to see your organized meetings and notes.\n",
                Utc::now().to_rfc3339()
            );
            fs::write(&index_path, index_content)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Create meeting template if it doesn't exist
        let meeting_template = templates.join("meeting.md");
        if !meeting_template.exists() {
            let template = "---\ntitle: \"{{title}}\"\ndate: \"{{date}}\"\ntype: meeting\ntags: [meeting]\nmeeting_id: \"{{meeting_id}}\"\nduration: {{duration}}\n---\n\n# {{title}}\n\n## Summary\n\n{{summary}}\n\n## Key Topics\n\n{{key_topics}}\n\n## Action Items\n\n{{action_items}}\n\n## Transcript\n\n{{transcript}}\n";
            fs::write(&meeting_template, template)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Create topic template
        let topic_template = templates.join("topic.md");
        if !topic_template.exists() {
            let template = "---\ntitle: \"{{title}}\"\ncreated: \"{{created}}\"\ntype: topic\ntags: []\n---\n\n# {{title}}\n\nOrganize your meetings, notes, and files under this topic.\n\n## Meetings\n\n## Notes\n";
            fs::write(&topic_template, template)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// List all topics (top-level folders under topics/)
    pub async fn list_topics(&self) -> Result<Vec<VaultTopic>, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let topics_dir = root.join("topics");

        if !topics_dir.exists() {
            self.ensure_structure().await?;
            return Ok(vec![]);
        }

        let mut topics = Vec::new();
        let mut entries = fs::read_dir(&topics_dir).await.map_err(|e| e.to_string())?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Count meetings
            let meetings_dir = path.join("meetings");
            let mut meeting_names = Vec::new();
            if meetings_dir.exists() {
                if let Ok(mut m_entries) = fs::read_dir(&meetings_dir).await {
                    while let Ok(Some(m_entry)) = m_entries.next_entry().await {
                        if m_entry.path().is_dir() {
                            if let Some(n) = m_entry.path().file_name() {
                                meeting_names.push(n.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }

            // Count notes
            let notes_dir = path.join("notes");
            let mut note_count = 0;
            if notes_dir.exists() {
                if let Ok(mut n_entries) = fs::read_dir(&notes_dir).await {
                    while let Ok(Some(_)) = n_entries.next_entry().await {
                        note_count += 1;
                    }
                }
            }

            // Read tags from _index.md frontmatter
            let mut tags = Vec::new();
            let mut created_at = String::new();
            let index_path = path.join("_index.md");
            if index_path.exists() {
                if let Ok(content) = fs::read_to_string(&index_path).await {
                    let (fm, _) = parse_frontmatter(&content);
                    if let Some(t) = fm.get("tags").and_then(|v| v.as_array()) {
                        tags = t
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }
                    if let Some(c) = fm.get("created").and_then(|v| v.as_str()) {
                        created_at = c.to_string();
                    }
                }
            }

            topics.push(VaultTopic {
                name,
                path: path.to_string_lossy().to_string(),
                meetings: meeting_names,
                note_count,
                created_at,
                tags,
            });
        }

        topics.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(topics)
    }

    /// Get a single topic with full details
    pub async fn get_topic(&self, topic_name: &str) -> Result<VaultTopic, String> {
        let topics = self.list_topics().await?;
        topics
            .into_iter()
            .find(|t| t.name == topic_name)
            .ok_or_else(|| format!("Topic '{}' not found", topic_name))
    }

    /// Create a new topic
    pub async fn create_topic(&self, name: &str, tags: Vec<String>) -> Result<VaultTopic, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        self.ensure_structure().await?;

        let topic_dir = root.join("topics").join(name);
        if topic_dir.exists() {
            return Err(format!("Topic '{}' already exists", name));
        }

        fs::create_dir_all(&topic_dir)
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(topic_dir.join("meetings"))
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(topic_dir.join("notes"))
            .await
            .map_err(|e| e.to_string())?;

        let now = Utc::now().to_rfc3339();
        let tags_str = tags
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(", ");
        let index_content = format!(
            "---\ntitle: \"{}\"\ncreated: \"{}\"\ntype: topic\ntags: [{}]\n---\n\n# {}\n\nOrganize your meetings, notes, and files under this topic.\n\n## Meetings\n\n## Notes\n",
            name, now, tags_str, name
        );
        fs::write(topic_dir.join("_index.md"), &index_content)
            .await
            .map_err(|e| e.to_string())?;

        Ok(VaultTopic {
            name: name.to_string(),
            path: topic_dir.to_string_lossy().to_string(),
            meetings: vec![],
            note_count: 0,
            created_at: now,
            tags,
        })
    }

    /// Export a meeting to the vault under a topic
    pub async fn export_meeting(
        &self,
        topic_name: &str,
        meeting_id: &str,
        title: &str,
        started_at: &str,
        duration_secs: Option<i64>,
        transcripts: &[(String, Option<String>, String)], // (text, speaker, timestamp)
        summary: Option<&str>,
        key_topics: Option<&str>,
        action_items: Option<&str>,
        screenshot_paths: &[String],
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let topic_dir = root.join("topics").join(topic_name);
        if !topic_dir.exists() {
            self.create_topic(topic_name, vec![]).await?;
        }

        // Create meeting folder named by date + title
        let safe_title = title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-");
        let date_prefix = &started_at[..10]; // YYYY-MM-DD
        let meeting_folder_name = format!("{}-{}", date_prefix, safe_title);
        let meeting_dir = topic_dir.join("meetings").join(&meeting_folder_name);
        fs::create_dir_all(&meeting_dir)
            .await
            .map_err(|e| e.to_string())?;

        // Build transcript markdown
        let mut transcript_md = String::new();
        for (text, speaker, timestamp) in transcripts {
            let time_str = &timestamp[11..19]; // HH:MM:SS
            match speaker {
                Some(s) => {
                    transcript_md.push_str(&format!("**[{}] {}:** {}\n\n", time_str, s, text))
                }
                None => transcript_md.push_str(&format!("**[{}]** {}\n\n", time_str, text)),
            }
        }

        // Build meeting markdown
        let duration_str = duration_secs
            .map(|d| format!("{}m {}s", d / 60, d % 60))
            .unwrap_or_else(|| "Unknown".to_string());

        let mut content = format!(
            "---\ntitle: \"{}\"\ndate: \"{}\"\ntype: meeting\ntags: [meeting]\nmeeting_id: \"{}\"\nduration: \"{}\"\n---\n\n# {}\n\n",
            title, started_at, meeting_id, duration_str, title
        );

        if let Some(s) = summary {
            content.push_str(&format!("## Summary\n\n{}\n\n", s));
        }
        if let Some(kt) = key_topics {
            content.push_str(&format!("## Key Topics\n\n{}\n\n", kt));
        }
        if let Some(ai) = action_items {
            content.push_str(&format!("## Action Items\n\n{}\n\n", ai));
        }

        content.push_str("## Transcript\n\n");
        content.push_str(&transcript_md);

        // Write main meeting file
        fs::write(meeting_dir.join("meeting.md"), &content)
            .await
            .map_err(|e| e.to_string())?;

        // Write standalone transcript
        if !transcript_md.is_empty() {
            let transcript_file = format!(
                "---\ntitle: \"Transcript - {}\"\ndate: \"{}\"\ntype: transcript\nmeeting_id: \"{}\"\n---\n\n# Transcript\n\n{}",
                title, started_at, meeting_id, transcript_md
            );
            fs::write(meeting_dir.join("transcript.md"), &transcript_file)
                .await
                .map_err(|e| e.to_string())?;
        }

        // Copy screenshots
        if !screenshot_paths.is_empty() {
            let screenshots_dir = meeting_dir.join("screenshots");
            fs::create_dir_all(&screenshots_dir)
                .await
                .map_err(|e| e.to_string())?;
            for (i, src_path) in screenshot_paths.iter().enumerate() {
                let src = Path::new(src_path);
                if src.exists() {
                    let ext = src
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_else(|| "png".to_string());
                    let dest = screenshots_dir.join(format!("{:04}.{}", i + 1, ext));
                    if let Err(e) = fs::copy(src, &dest).await {
                        log::warn!("Failed to copy screenshot {}: {}", src_path, e);
                    }
                }
            }
        }

        // Update topic _index.md with meeting link
        let index_path = topic_dir.join("_index.md");
        if index_path.exists() {
            if let Ok(mut index_content) = fs::read_to_string(&index_path).await {
                let link = format!("\n- [[meetings/{}/meeting|{}]]", meeting_folder_name, title);
                if !index_content.contains(&meeting_folder_name) {
                    index_content.push_str(&link);
                    let _ = fs::write(&index_path, &index_content).await;
                }
            }
        }

        Ok(meeting_dir.to_string_lossy().to_string())
    }

    /// Read a file from the vault
    pub async fn read_file(&self, file_path: &str) -> Result<VaultFileContent, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let full_path = if Path::new(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            vault.join(file_path)
        };

        if !full_path.exists() {
            return Err(format!("File not found: {}", file_path));
        }

        // Security: ensure path is within vault
        if !full_path.starts_with(&vault) {
            return Err("Access denied: path is outside vault".to_string());
        }

        let content = fs::read_to_string(&full_path)
            .await
            .map_err(|e| e.to_string())?;
        let (frontmatter, body) = parse_frontmatter(&content);

        Ok(VaultFileContent {
            path: full_path.to_string_lossy().to_string(),
            content,
            frontmatter,
            body,
        })
    }

    /// Write a note to a topic
    pub async fn write_note(
        &self,
        topic_name: &str,
        file_name: &str,
        content: &str,
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let notes_dir = root.join("topics").join(topic_name).join("notes");
        fs::create_dir_all(&notes_dir)
            .await
            .map_err(|e| e.to_string())?;

        let safe_name = if file_name.ends_with(".md") {
            file_name.to_string()
        } else {
            format!("{}.md", file_name)
        };

        let dest = notes_dir.join(&safe_name);
        fs::write(&dest, content).await.map_err(|e| e.to_string())?;
        Ok(dest.to_string_lossy().to_string())
    }

    /// Upload (copy) a file into a topic
    pub async fn upload_file(
        &self,
        topic_name: &str,
        source_path: &str,
        dest_name: Option<&str>,
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let notes_dir = root.join("topics").join(topic_name).join("notes");
        fs::create_dir_all(&notes_dir)
            .await
            .map_err(|e| e.to_string())?;

        let src = Path::new(source_path);
        if !src.exists() {
            return Err(format!("Source file not found: {}", source_path));
        }

        let filename = dest_name.map(String::from).unwrap_or_else(|| {
            src.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "uploaded".to_string())
        });

        let dest = notes_dir.join(&filename);
        fs::copy(src, &dest).await.map_err(|e| e.to_string())?;
        Ok(dest.to_string_lossy().to_string())
    }

    /// List files recursively
    pub async fn list_files(&self, sub_path: Option<&str>) -> Result<Vec<VaultFile>, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let base = match sub_path {
            Some(sp) => vault.join(sp),
            None => vault.join("noFriction"),
        };

        if !base.exists() {
            return Ok(vec![]);
        }

        let mut files = Vec::new();
        collect_files(&base, &vault, &mut files).await?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Build a tree structure of the vault
    pub async fn get_tree(&self) -> Result<VaultTreeNode, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let nf_root = vault.join("noFriction");

        if !nf_root.exists() {
            self.ensure_structure().await?;
        }

        build_tree(&nf_root).await
    }

    /// Search through vault markdown files
    pub async fn search(&self, query: &str) -> Result<Vec<VaultSearchResult>, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let nf_root = vault.join("noFriction");

        if !nf_root.exists() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        search_files(&nf_root, &query_lower, &mut results).await?;
        Ok(results)
    }

    /// Delete a file or folder from the vault
    pub async fn delete_item(&self, item_path: &str) -> Result<(), String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let full_path = PathBuf::from(item_path);

        // Security check
        if !full_path.starts_with(&vault) {
            return Err("Access denied: path is outside vault".to_string());
        }

        if full_path.is_dir() {
            fs::remove_dir_all(&full_path)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&full_path)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// ─── Helper Functions ──────────────────────────────────────────────

/// Parse YAML frontmatter from markdown content
fn parse_frontmatter(content: &str) -> (serde_json::Value, String) {
    if !content.starts_with("---") {
        return (
            serde_json::Value::Object(serde_json::Map::new()),
            content.to_string(),
        );
    }

    let rest = &content[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let yaml_str = &rest[..end_idx].trim();
        let body = rest[end_idx + 4..].trim_start().to_string();

        // Parse YAML into JSON value
        match serde_yaml::from_str::<serde_json::Value>(yaml_str) {
            Ok(val) => (val, body),
            Err(_) => (
                serde_json::Value::Object(serde_json::Map::new()),
                content.to_string(),
            ),
        }
    } else {
        (
            serde_json::Value::Object(serde_json::Map::new()),
            content.to_string(),
        )
    }
}

/// Count files recursively
async fn count_files_recursive(dir: &Path) -> Result<i32, String> {
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        if let Ok(mut entries) = fs::read_dir(&current).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories
                    if !path
                        .file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                    {
                        stack.push(path);
                    }
                } else {
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Collect files recursively into a flat list
async fn collect_files(
    dir: &Path,
    vault_root: &Path,
    files: &mut Vec<VaultFile>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir).await.map_err(|e| e.to_string())?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden files
        if name.starts_with('.') {
            continue;
        }

        let metadata = entry.metadata().await.map_err(|e| e.to_string())?;
        let modified = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
            .unwrap_or_default();

        let relative = path
            .strip_prefix(vault_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let extension = path.extension().map(|e| e.to_string_lossy().to_string());

        files.push(VaultFile {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            relative_path: relative,
            is_dir: path.is_dir(),
            modified,
            size: metadata.len(),
            extension,
        });

        if path.is_dir() {
            Box::pin(collect_files(&path, vault_root, files)).await?;
        }
    }
    Ok(())
}

/// Build tree structure recursively
async fn build_tree(dir: &Path) -> Result<VaultTreeNode, String> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "noFriction".to_string());

    let mut children = Vec::new();

    if dir.is_dir() {
        let mut entries = fs::read_dir(dir).await.map_err(|e| e.to_string())?;
        let mut child_items = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let child_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if child_name.starts_with('.') {
                continue;
            }

            child_items.push(path);
        }

        child_items.sort();

        for child_path in child_items {
            if child_path.is_dir() {
                children.push(Box::pin(build_tree(&child_path)).await?);
            } else {
                children.push(VaultTreeNode {
                    name: child_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path: child_path.to_string_lossy().to_string(),
                    is_dir: false,
                    children: vec![],
                });
            }
        }
    }

    Ok(VaultTreeNode {
        name,
        path: dir.to_string_lossy().to_string(),
        is_dir: true,
        children,
    })
}

/// Search files recursively for query matches
async fn search_files(
    dir: &Path,
    query: &str,
    results: &mut Vec<VaultSearchResult>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir).await.map_err(|e| e.to_string())?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            Box::pin(search_files(&path, query, results)).await?;
        } else if name.ends_with(".md") {
            if let Ok(content) = fs::read_to_string(&path).await {
                for (line_num, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(query) {
                        // Get surrounding context (1 line before and after)
                        let lines: Vec<&str> = content.lines().collect();
                        let start = if line_num > 0 { line_num - 1 } else { 0 };
                        let end = (line_num + 2).min(lines.len());
                        let context = lines[start..end].join("\n");

                        results.push(VaultSearchResult {
                            file_path: path.to_string_lossy().to_string(),
                            file_name: name.clone(),
                            matching_line: line.to_string(),
                            line_number: line_num + 1,
                            context,
                        });

                        if results.len() >= 100 {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
