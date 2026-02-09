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

/// A wikilink found in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultLink {
    pub source_file: String,
    pub target: String,
    pub display_text: String,
    pub line_number: usize,
}

/// Backlinks pointing to a specific file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklinkResult {
    pub target_file: String,
    pub backlinks: Vec<VaultLink>,
}

/// A tag with usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultTag {
    pub name: String,
    pub file_count: i32,
    pub files: Vec<String>,
}

/// Node in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub file_type: String,
}

/// Edge in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// Complete graph structure for visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
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
        let people = root.join("people");
        let companies = root.join("companies");

        fs::create_dir_all(&topics)
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(&templates)
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(&people)
            .await
            .map_err(|e| e.to_string())?;
        fs::create_dir_all(&companies)
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
        intelligence: Option<&str>,
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

        if let Some(intel) = intelligence {
            content.push_str("## AI Intelligence\n\n");
            content.push_str(intel);
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

    // ═══════════════════════════════════════════════════════════════════
    // People & Company Intelligence APIs
    // ═══════════════════════════════════════════════════════════════════

    /// Write or update a person note in the people/ directory
    pub async fn write_person_note(
        &self,
        name: &str,
        email: &str,
        company: &str,
        briefing: &str,
        meeting_links: &[String],
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let people_dir = root.join("people");
        fs::create_dir_all(&people_dir)
            .await
            .map_err(|e| e.to_string())?;

        // Sanitize filename
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let file_path = people_dir.join(format!("{}.md", safe_name));

        // Build meeting links section
        let meetings_section = if meeting_links.is_empty() {
            "*No meetings recorded yet*".to_string()
        } else {
            meeting_links
                .iter()
                .map(|link| format!("- [[{}]]", link))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Build the note content
        let content = format!(
            "---\ntitle: \"{}\"\nemail: \"{}\"\ncompany: \"[[{}]]\"\ntype: person\ntags: [person, contact]\nlast_updated: \"{}\"\n---\n\n# {}\n\n📧 {} | 🏢 [[{}]]\n\n## Briefing\n\n{}\n\n## Meetings\n\n{}\n",
            name,
            email,
            company,
            Utc::now().format("%Y-%m-%d"),
            name,
            email,
            company,
            briefing,
            meetings_section
        );

        fs::write(&file_path, &content)
            .await
            .map_err(|e| e.to_string())?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// Write or update a company note in the companies/ directory
    pub async fn write_company_note(
        &self,
        company_name: &str,
        domain: &str,
        briefing: &str,
        people_names: &[String],
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let companies_dir = root.join("companies");
        fs::create_dir_all(&companies_dir)
            .await
            .map_err(|e| e.to_string())?;

        // Sanitize filename
        let safe_name: String = company_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let file_path = companies_dir.join(format!("{}.md", safe_name));

        // Build people section with wikilinks
        let people_section = if people_names.is_empty() {
            "*No contacts recorded yet*".to_string()
        } else {
            people_names
                .iter()
                .map(|name| format!("- [[{}]]", name))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let content = format!(
            "---\ntitle: \"{}\"\ndomain: \"{}\"\ntype: company\ntags: [company]\nlast_updated: \"{}\"\n---\n\n# {}\n\n🌐 {}\n\n## Overview\n\n{}\n\n## People\n\n{}\n",
            company_name,
            domain,
            Utc::now().format("%Y-%m-%d"),
            company_name,
            domain,
            briefing,
            people_section
        );

        fs::write(&file_path, &content)
            .await
            .map_err(|e| e.to_string())?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// Write a meeting prep document under a topic
    pub async fn write_meeting_prep(
        &self,
        topic_name: &str,
        event_title: &str,
        event_date: &str,
        attendee_names: &[String],
        meeting_prep_content: &str,
    ) -> Result<String, String> {
        let root = self.nofriction_root().ok_or("Vault path not configured")?;
        let topic_dir = root.join("topics").join(topic_name);
        fs::create_dir_all(&topic_dir)
            .await
            .map_err(|e| e.to_string())?;

        // Build attendee wikilinks
        let attendees_md = attendee_names
            .iter()
            .map(|name| format!("- [[{}]]", name))
            .collect::<Vec<_>>()
            .join("\n");

        let safe_date = event_date.chars().take(10).collect::<String>();
        let file_name = format!("Meeting Prep - {} - {}.md", event_title, safe_date);
        let file_path = topic_dir.join(&file_name);

        let content = format!(
            "---\ntitle: \"Meeting Prep - {}\"\ndate: \"{}\"\ntype: meeting-prep\ntags: [meeting-prep, intel]\n---\n\n# 🎯 Meeting Prep: {}\n\n📅 {}\n\n## Attendees\n\n{}\n\n## Intelligence Brief\n\n{}\n",
            event_title,
            event_date,
            event_title,
            event_date,
            attendees_md,
            meeting_prep_content
        );

        fs::write(&file_path, &content)
            .await
            .map_err(|e| e.to_string())?;

        Ok(file_path.to_string_lossy().to_string())
    }

    // ═══════════════════════════════════════════════════════════════════
    // Obsidian-style Knowledge Management APIs
    // ═══════════════════════════════════════════════════════════════════

    /// Extract wikilinks from markdown content
    pub fn extract_wikilinks(content: &str, source_file: &str) -> Vec<VaultLink> {
        let mut links = Vec::new();
        let re = regex::Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap();

        for (line_num, line) in content.lines().enumerate() {
            for cap in re.captures_iter(line) {
                let target = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let display = cap.get(2).map(|m| m.as_str()).unwrap_or(target);

                links.push(VaultLink {
                    source_file: source_file.to_string(),
                    target: target.to_string(),
                    display_text: display.to_string(),
                    line_number: line_num + 1,
                });
            }
        }
        links
    }

    /// Get all backlinks pointing to a specific file
    pub async fn get_backlinks(&self, target_file: &str) -> Result<BacklinkResult, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let nf_root = vault.join("noFriction");

        if !nf_root.exists() {
            return Ok(BacklinkResult {
                target_file: target_file.to_string(),
                backlinks: vec![],
            });
        }

        // Extract target name for matching (without extension)
        let target_name = Path::new(target_file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut backlinks = Vec::new();
        collect_backlinks(&nf_root, &target_name, &mut backlinks).await?;

        Ok(BacklinkResult {
            target_file: target_file.to_string(),
            backlinks,
        })
    }

    /// List all tags in the vault with file counts
    pub async fn list_tags(&self) -> Result<Vec<VaultTag>, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let nf_root = vault.join("noFriction");

        if !nf_root.exists() {
            return Ok(vec![]);
        }

        let mut tag_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        collect_tags(&nf_root, &mut tag_map).await?;

        let mut tags: Vec<VaultTag> = tag_map
            .into_iter()
            .map(|(name, files)| VaultTag {
                name,
                file_count: files.len() as i32,
                files,
            })
            .collect();

        tags.sort_by(|a, b| b.file_count.cmp(&a.file_count));
        Ok(tags)
    }

    /// Get files that have a specific tag
    pub async fn get_files_by_tag(&self, tag: &str) -> Result<Vec<VaultFile>, String> {
        let tags = self.list_tags().await?;
        let tag_lower = tag.to_lowercase().trim_start_matches('#').to_string();

        if let Some(vault_tag) = tags.iter().find(|t| t.name.to_lowercase() == tag_lower) {
            let vault = self.get_vault_path().ok_or("Vault path not configured")?;
            let mut files = Vec::new();

            for file_path in &vault_tag.files {
                let path = Path::new(file_path);
                if path.exists() {
                    if let Ok(metadata) = tokio::fs::metadata(path).await {
                        let modified = metadata
                            .modified()
                            .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
                            .unwrap_or_default();

                        files.push(VaultFile {
                            name: path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            path: file_path.clone(),
                            relative_path: path
                                .strip_prefix(&vault)
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            is_dir: false,
                            modified,
                            size: metadata.len(),
                            extension: path.extension().map(|e| e.to_string_lossy().to_string()),
                        });
                    }
                }
            }
            Ok(files)
        } else {
            Ok(vec![])
        }
    }

    /// Build the knowledge graph for visualization
    pub async fn build_graph(&self) -> Result<VaultGraph, String> {
        let vault = self.get_vault_path().ok_or("Vault path not configured")?;
        let nf_root = vault.join("noFriction");

        if !nf_root.exists() {
            return Ok(VaultGraph {
                nodes: vec![],
                edges: vec![],
            });
        }

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut file_ids: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Collect all markdown files as nodes
        collect_graph_nodes(&nf_root, &mut nodes, &mut file_ids).await?;

        // Collect all edges (wikilinks)
        collect_graph_edges(&nf_root, &file_ids, &mut edges).await?;

        Ok(VaultGraph { nodes, edges })
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

// ─── Obsidian Knowledge Graph Helpers ──────────────────────────────

/// Extract wikilinks from content using regex
fn extract_wikilinks_from_content(content: &str, source_file: &str) -> Vec<VaultLink> {
    let mut links = Vec::new();
    let re = regex::Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for cap in re.captures_iter(line) {
            let target = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let display = cap.get(2).map(|m| m.as_str()).unwrap_or(target);

            links.push(VaultLink {
                source_file: source_file.to_string(),
                target: target.to_string(),
                display_text: display.to_string(),
                line_number: line_num + 1,
            });
        }
    }
    links
}

/// Collect backlinks recursively
async fn collect_backlinks(
    dir: &Path,
    target_name: &str,
    backlinks: &mut Vec<VaultLink>,
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
            Box::pin(collect_backlinks(&path, target_name, backlinks)).await?;
        } else if name.ends_with(".md") {
            if let Ok(content) = fs::read_to_string(&path).await {
                let links = extract_wikilinks_from_content(&content, &path.to_string_lossy());
                for link in links {
                    // Match target by name (case-insensitive)
                    if link.target.to_lowercase() == target_name.to_lowercase()
                        || link
                            .target
                            .to_lowercase()
                            .ends_with(&format!("/{}", target_name.to_lowercase()))
                    {
                        backlinks.push(link);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Collect tags from files recursively
async fn collect_tags(
    dir: &Path,
    tag_map: &mut std::collections::HashMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir).await.map_err(|e| e.to_string())?;
    let tag_re = regex::Regex::new(r"#([a-zA-Z][a-zA-Z0-9_-]*)").unwrap();

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
            Box::pin(collect_tags(&path, tag_map)).await?;
        } else if name.ends_with(".md") {
            if let Ok(content) = fs::read_to_string(&path).await {
                let file_path = path.to_string_lossy().to_string();

                // Extract tags from frontmatter
                let (frontmatter, body) = parse_frontmatter(&content);
                if let Some(tags) = frontmatter.get("tags").and_then(|v| v.as_array()) {
                    for tag in tags {
                        if let Some(tag_str) = tag.as_str() {
                            let normalized = tag_str.trim_start_matches('#').to_string();
                            tag_map
                                .entry(normalized)
                                .or_default()
                                .push(file_path.clone());
                        }
                    }
                }

                // Extract inline #tags from body
                for cap in tag_re.captures_iter(&body) {
                    if let Some(tag) = cap.get(1) {
                        let normalized = tag.as_str().to_string();
                        let files = tag_map.entry(normalized).or_default();
                        if !files.contains(&file_path) {
                            files.push(file_path.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Collect graph nodes (markdown files)
async fn collect_graph_nodes(
    dir: &Path,
    nodes: &mut Vec<GraphNode>,
    file_ids: &mut std::collections::HashMap<String, String>,
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
            Box::pin(collect_graph_nodes(&path, nodes, file_ids)).await?;
        } else if name.ends_with(".md") {
            let file_path = path.to_string_lossy().to_string();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            // Determine file type from path or frontmatter
            let file_type = if file_path.contains("/meetings/") {
                "meeting"
            } else if file_path.contains("/topics/") && name == "_index.md" {
                "topic"
            } else {
                "note"
            };

            let node_id = stem.clone();
            file_ids.insert(stem.to_lowercase(), node_id.clone());

            nodes.push(GraphNode {
                id: node_id,
                label: stem,
                file_type: file_type.to_string(),
            });
        }
    }
    Ok(())
}

/// Collect graph edges (wikilinks between files)
async fn collect_graph_edges(
    dir: &Path,
    file_ids: &std::collections::HashMap<String, String>,
    edges: &mut Vec<GraphEdge>,
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
            Box::pin(collect_graph_edges(&path, file_ids, edges)).await?;
        } else if name.ends_with(".md") {
            if let Ok(content) = fs::read_to_string(&path).await {
                let source_stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let links = extract_wikilinks_from_content(&content, &path.to_string_lossy());

                for link in links {
                    let target_key = link.target.to_lowercase();
                    // Find target node ID
                    if let Some(target_id) = file_ids.get(&target_key) {
                        edges.push(GraphEdge {
                            source: source_stem.clone(),
                            target: target_id.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
