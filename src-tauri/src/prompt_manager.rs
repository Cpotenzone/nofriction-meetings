// noFriction Meetings - Prompt Manager
// Manages prompt library, model configurations, and use case mappings

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

// ============================================
// Data Structures
// ============================================

/// A prompt in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub system_prompt: String,
    pub user_prompt_template: Option<String>,
    pub model_id: Option<String>,
    pub temperature: f32,
    pub max_tokens: Option<i32>,
    pub theme: Option<String>, // NEW: For theme-specific prompts (prospecting, fundraising, etc.)
    pub version: i32,          // NEW: Version control for prompt evolution
    pub is_builtin: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCreate {
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub system_prompt: String,
    pub user_prompt_template: Option<String>,
    pub model_id: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
}

/// Input for updating a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub system_prompt: Option<String>,
    pub user_prompt_template: Option<String>,
    pub model_id: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_active: Option<bool>,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub model_type: String, // "llm" or "vlm"
    pub base_url: String,
    pub capabilities: Vec<String>,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
    pub is_available: bool,
    pub last_health_check: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Input for creating model config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigCreate {
    pub name: String,
    pub display_name: String,
    pub model_type: String,
    pub base_url: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub default_temperature: Option<f32>,
    pub default_max_tokens: Option<i32>,
}

/// Use case mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseCase {
    pub id: String,
    pub use_case: String,
    pub display_name: String,
    pub description: Option<String>,
    pub prompt_id: Option<String>,
    pub model_id: Option<String>,
    pub priority: i32,
    pub conditions: Option<String>, // JSON conditions
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Use case with resolved prompt and model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedUseCase {
    pub use_case: UseCase,
    pub prompt: Option<Prompt>,
    pub model: Option<ModelConfig>,
}

// ============================================
// Prompt Manager
// ============================================

#[derive(Clone)]
pub struct PromptManager {
    pool: Pool<Sqlite>,
}

impl PromptManager {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Run migrations for prompt management tables
    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create prompt_library table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS prompt_library (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                category TEXT NOT NULL DEFAULT 'general',
                system_prompt TEXT NOT NULL,
                user_prompt_template TEXT,
                model_id TEXT,
                temperature REAL NOT NULL DEFAULT 0.5,
                max_tokens INTEGER,
                theme TEXT,
                version INTEGER NOT NULL DEFAULT 1,
                is_builtin BOOLEAN NOT NULL DEFAULT 0,
                is_active BOOLEAN NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Add indexes for theme and version
        let _ =
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_prompts_theme ON prompt_library(theme)")
                .execute(&self.pool)
                .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_prompts_version ON prompt_library(name, version DESC)",
        )
        .execute(&self.pool)
        .await;

        // Create model_configurations table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS model_configurations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                model_type TEXT NOT NULL DEFAULT 'llm',
                base_url TEXT NOT NULL DEFAULT 'http://localhost:8080',
                capabilities TEXT,
                default_temperature REAL NOT NULL DEFAULT 0.5,
                default_max_tokens INTEGER NOT NULL DEFAULT 2048,
                is_available BOOLEAN NOT NULL DEFAULT 0,
                last_health_check TEXT,
                created_at TEXT NOT NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Create use_case_mappings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS use_case_mappings (
                id TEXT PRIMARY KEY,
                use_case TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                description TEXT,
                prompt_id TEXT REFERENCES prompt_library(id),
                model_id TEXT REFERENCES model_configurations(id),
                priority INTEGER NOT NULL DEFAULT 0,
                conditions TEXT,
                is_active BOOLEAN NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        // Seed default data
        self.seed_defaults().await?;

        Ok(())
    }

    /// Seed default prompts, models, and use cases
    async fn seed_defaults(&self) -> Result<(), Box<dyn std::error::Error>> {
        let now = Utc::now().to_rfc3339();

        // Check if already seeded
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM prompt_library WHERE is_builtin = 1")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        if count > 0 {
            return Ok(());
        }

        // Seed default models
        let models = vec![
            (
                "qwen2.5vl:7b",
                "Qwen 2.5 VL 7B",
                "vlm",
                vec!["vision", "text", "chat", "image-analysis"],
            ),
            (
                "qwen2.5vl:3b",
                "Qwen 2.5 VL 3B",
                "vlm",
                vec!["vision", "text", "chat", "fast"],
            ),
        ];

        for (name, display, model_type, caps) in models {
            let id = Uuid::new_v4().to_string();
            let caps_json = serde_json::to_string(&caps).unwrap_or_default();
            sqlx::query(r#"
                INSERT OR IGNORE INTO model_configurations 
                (id, name, display_name, model_type, base_url, capabilities, default_temperature, default_max_tokens, is_available, created_at)
                VALUES (?, ?, ?, ?, 'http://localhost:8080', ?, 0.5, 2048, 0, ?)
            "#)
            .bind(&id)
            .bind(name)
            .bind(display)
            .bind(model_type)
            .bind(&caps_json)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }

        // Get model IDs
        let llm_model_id: Option<String> =
            sqlx::query_scalar("SELECT id FROM model_configurations WHERE name = 'qwen2.5vl:7b'")
                .fetch_optional(&self.pool)
                .await?;
        let vlm_model_id: Option<String> =
            sqlx::query_scalar("SELECT id FROM model_configurations WHERE name = 'qwen2.5vl:7b'")
                .fetch_optional(&self.pool)
                .await?;

        // Seed default prompts
        let prompts = vec![
            (
                "meeting_summary",
                "Meeting Summary",
                "Generate a concise summary of meeting content",
                "meeting",
                r#"You are a professional meeting assistant. Your task is to summarize meeting content concisely and accurately.

Focus on:
- Key discussion points
- Decisions made
- Important numbers, dates, or commitments mentioned
- Overall sentiment and tone

Keep summaries clear and actionable. Use bullet points when appropriate."#,
                0.3,
                &llm_model_id,
            ),
            (
                "action_items",
                "Extract Action Items",
                "Identify tasks and commitments from meeting content",
                "meeting",
                r#"You are a task extraction assistant. Your job is to identify action items, tasks, and commitments from meeting content.

For each action item, extract:
- The task description
- Who is responsible (if mentioned)
- Due date or timeline (if mentioned)
- Priority level based on context

Format as a clear, actionable checklist."#,
                0.2,
                &llm_model_id,
            ),
            (
                "qa_assistant",
                "Q&A Assistant",
                "Answer questions about meeting content",
                "meeting",
                r#"You are a helpful meeting assistant with access to meeting transcripts and screen content.
Answer questions based solely on the meeting content provided. If the answer isn't in the content, say so.
Be precise and cite specific parts of the meeting when relevant."#,
                0.5,
                &llm_model_id,
            ),
            (
                "frame_analysis",
                "Frame Analysis",
                "Analyze screen captures for activity context",
                "vlm",
                r#"You are analyzing a screenshot from a user's computer during a work session.

Describe:
1. What application or website is visible
2. What the user appears to be doing
3. Any readable text or important content
4. The overall context of this work moment

Be concise but thorough. Focus on actionable insights."#,
                0.4,
                &vlm_model_id,
            ),
            (
                "code_review",
                "Code Review",
                "Analyze code visible in screenshots",
                "vlm",
                r#"You are a senior developer reviewing code visible in this screenshot.

Analyze:
1. What programming language is being used
2. What the code appears to do
3. Any potential issues or improvements
4. Code quality observations

Be constructive and specific."#,
                0.3,
                &vlm_model_id,
            ),
            (
                "presentation_notes",
                "Presentation Notes",
                "Extract notes from slide presentations",
                "vlm",
                r#"You are analyzing a presentation slide visible in this screenshot.

Extract:
1. The slide title or main topic
2. Key bullet points or content
3. Any important data, charts, or figures
4. Speaker notes if visible

Format as clean, organized notes."#,
                0.3,
                &vlm_model_id,
            ),
            (
                "focus_detection",
                "Focus Detection",
                "Identify the user's current work focus",
                "vlm",
                r#"Based on this screenshot, determine the user's current work focus.

Categorize into one of:
- Coding/Development
- Writing/Documentation
- Communication (email, chat, meetings)
- Research/Reading
- Design/Creative
- Data Analysis
- Administrative
- Other

Provide a brief one-sentence summary of the specific task."#,
                0.3,
                &vlm_model_id,
            ),
        ];

        for (id, name, desc, category, system_prompt, temp, model_id) in prompts {
            let prompt_id = Uuid::new_v4().to_string();
            sqlx::query(r#"
                INSERT OR IGNORE INTO prompt_library 
                (id, name, description, category, system_prompt, model_id, temperature, theme, version, is_builtin, is_active, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, NULL, 1, 1, 1, ?, ?)
            "#)
            .bind(&prompt_id)
            .bind(name)
            .bind(desc)
            .bind(category)
            .bind(system_prompt)
            .bind(model_id)
            .bind(temp)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;

            // Create use case mapping for this prompt
            let use_case_id = Uuid::new_v4().to_string();
            sqlx::query(r#"
                INSERT OR IGNORE INTO use_case_mappings 
                (id, use_case, display_name, description, prompt_id, model_id, priority, is_active, created_at)
                VALUES (?, ?, ?, ?, ?, ?, 0, 1, ?)
            "#)
            .bind(&use_case_id)
            .bind(id)
            .bind(name)
            .bind(desc)
            .bind(&prompt_id)
            .bind(model_id)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }

        // ===================================================================
        // Phase 2: Seed Theme-Specific Prompts
        // ===================================================================

        let vlm_id = vlm_model_id.as_ref();

        // PROSPECTING THEME
        self.create_theme_prompt(
            "prospecting",
            "prospecting_entity_extraction",
            "Extract people, companies, and outreach activity from prospecting sessions",
            "vlm",
            r#"You are analyzing screen/audio data during a prospecting session.

Extract the following entities in JSON format:
{
  "people": [{"name": "", "title": "", "company": "", "contact_info": ""}],
  "companies": [{"name": "", "industry": "", "size": ""}],
  "outreach_activities": [{"type": "email|call|message", "recipient": "", "sentiment": ""}],
  "meetings": [{"scheduled_with": "", "date": "", "purpose": ""}]
}

Assign confidence level (high/medium/low) to each entity."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.3),
        )
        .await?;

        self.create_theme_prompt(
            "prospecting",
            "prospecting_context_analysis",
            "Analyze prospecting activity stage and sentiment",
            "vlm",
            r#"Analyze this prospecting activity and determine:
1. Stage: research | initial_contact | follow_up | meeting_booked
2. Key talking points or value props mentioned
3. Sentiment of interactions (positive/neutral/negative)
4. Recommended next steps

Return structured JSON."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.4),
        )
        .await?;

        // FUNDRAISING THEME
        self.create_theme_prompt(
            "fundraising",
            "fundraising_entity_extraction",
            "Extract investors, pitch deck details, and feedback from fundraising activities",
            "vlm",
            r#"You are analyzing screen/audio data during fundraising activities.

Extract the following in JSON format:
{
  "investors": [{"name": "", "firm": "", "check_size": "", "stage_focus": ""}],
  "pitch_deck": {"version": "", "visible_slides": [], "changes_made": []},
  "feedback": [{"investor": "", "question": "", "concern": "", "positive_signal": ""}],
  "pipeline": [{"investor": "", "stage": "intro|pitch|diligence|offer"}]
}

Mark confidence level for each entity."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.3),
        )
        .await?;

        self.create_theme_prompt(
            "fundraising",
            "fundraising_context_analysis",
            "Analyze fundraising meeting outcomes and next steps",
            "vlm",
            r#"Analyze this fundraising activity:
1. Meeting type: intro | pitch | update | due_diligence
2. Investor sentiment: very_interested | interested | neutral | not_interested
3. Key objections or concerns raised
4. Next steps and timeline
5. Probability of investment (high/medium/low)

Return structured JSON."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.4),
        )
        .await?;

        // PRODUCT DEVELOPMENT THEME
        self.create_theme_prompt(
            "product_dev",
            "product_entity_extraction",
            "Extract features, decisions, and artifacts from product development work",
            "vlm",
            r#"Analyzing product development work.

Extract in JSON format:
{
  "features": [{"name": "", "description": "", "priority": "high|medium|low"}],
  "decisions": [{"type": "technical|design|product", "decision": "", "rationale": ""}],
  "artifacts": [{"type": "mockup|diagram|code", "description": "", "file_name": ""}],
  "collaborators": [{"name": "", "role": "", "contribution": ""}]
}

Assign confidence levels."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.3),
        )
        .await?;

        self.create_theme_prompt(
            "product_dev",
            "product_context_analysis",
            "Analyze product development focus and progress",
            "vlm",
            r#"Analyze this product development activity:
1. Focus area: feature_development | bug_fixing | refactoring | design | planning
2. Progress indicators: code_written | tests_added | reviewed | deployed
3. Blockers or challenges identified
4. Collaboration quality: solo | paired | team_discussion
5. Context switches detected (interruptions)

Return structured JSON."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.4),
        )
        .await?;

        // ADMIN THEME
        self.create_theme_prompt(
            "admin",
            "admin_entity_extraction",
            "Extract tasks, workflows, and automation opportunities from administrative work",
            "vlm",
            r#"Analyzing administrative work.

Extract in JSON format:
{
  "tasks": [{"description": "", "deadline": "", "priority": ""}],
  "workflows": [{"name": "", "steps": [], "frequency": ""}],
  "tools": [{"name": "", "purpose": "", "time_spent": ""}],
  "automation_opportunities": [{"workflow": "", "potential_savings": "", "complexity": "low|medium|high"}]
}

Mark confidence levels."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.3),
        ).await?;

        self.create_theme_prompt(
            "admin",
            "admin_context_analysis",
            "Analyze administrative burden and efficiency",
            "vlm",
            r#"Analyze this administrative activity:
1. Task type: email | scheduling | filing | reporting | other
2. Repetitiveness: one_time | weekly | daily | ad_hoc
3. Automation potential: high | medium | low | none
4. Time efficiency: optimal | acceptable | inefficient
5. Estimated time saved if automated

Return structured JSON."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.4),
        )
        .await?;

        // PERSONAL THEME
        self.create_theme_prompt(
            "personal",
            "personal_activity_categorization",
            "High-level activity categorization for personal time",
            "vlm",
            r#"High-level activity analysis. Categorize into one of:
- work_professional
- learning_development
- communication
- entertainment
- health_wellness
- other

Extract only high-confidence entities in JSON:
{
  "category": "",
  "subcategory": "",
  "tools_used": [],
  "duration_estimate": ""
}

Be conservative - only extract what you're confident about."#,
            vlm_id.map(|s| s.as_str()),
            Some(0.5),
        )
        .await?;

        log::info!("Seeded default prompts, models, use cases, and theme-specific prompts");
        Ok(())
    }

    // ============================================
    // Prompt CRUD Operations
    // ============================================

    pub async fn create_prompt(&self, input: PromptCreate) -> Result<Prompt, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(r#"
            INSERT INTO prompt_library 
            (id, name, description, category, system_prompt, user_prompt_template, model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, 0, 1, ?, ?)
        "#)
        .bind(&id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.category)
        .bind(&input.system_prompt)
        .bind(&input.user_prompt_template)
        .bind(&input.model_id)
        .bind(input.temperature.unwrap_or(0.5))
        .bind(&input.max_tokens)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        Ok(Prompt {
            id,
            name: input.name,
            description: input.description,
            category: input.category,
            system_prompt: input.system_prompt,
            user_prompt_template: input.user_prompt_template,
            model_id: input.model_id,
            temperature: input.temperature.unwrap_or(0.5),
            max_tokens: input.max_tokens,
            theme: None,
            version: 1,
            is_builtin: false,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_prompt(&self, id: &str) -> Result<Option<Prompt>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, category, system_prompt, user_prompt_template, 
                   model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
            FROM prompt_library WHERE id = ?
        "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| self.row_to_prompt(&r)))
    }

    pub async fn list_prompts(&self, category: Option<&str>) -> Result<Vec<Prompt>, sqlx::Error> {
        let query = if let Some(cat) = category {
            sqlx::query(r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library WHERE category = ? ORDER BY name
            "#)
            .bind(cat)
        } else {
            sqlx::query(
                r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library ORDER BY category, name
            "#,
            )
        };

        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| self.row_to_prompt(r)).collect())
    }

    pub async fn update_prompt(
        &self,
        id: &str,
        updates: PromptUpdate,
    ) -> Result<Option<Prompt>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        // Build dynamic update query
        let mut set_clauses = vec!["updated_at = ?".to_string()];
        let mut bindings: Vec<String> = vec![now.clone()];

        if let Some(name) = &updates.name {
            set_clauses.push("name = ?".to_string());
            bindings.push(name.clone());
        }
        if let Some(desc) = &updates.description {
            set_clauses.push("description = ?".to_string());
            bindings.push(desc.clone());
        }
        if let Some(cat) = &updates.category {
            set_clauses.push("category = ?".to_string());
            bindings.push(cat.clone());
        }
        if let Some(sp) = &updates.system_prompt {
            set_clauses.push("system_prompt = ?".to_string());
            bindings.push(sp.clone());
        }
        if let Some(upt) = &updates.user_prompt_template {
            set_clauses.push("user_prompt_template = ?".to_string());
            bindings.push(upt.clone());
        }
        if let Some(mid) = &updates.model_id {
            set_clauses.push("model_id = ?".to_string());
            bindings.push(mid.clone());
        }

        let query = format!(
            "UPDATE prompt_library SET {} WHERE id = ?",
            set_clauses.join(", ")
        );

        let mut q = sqlx::query(&query);
        for b in &bindings {
            q = q.bind(b);
        }
        if let Some(temp) = updates.temperature {
            q = q.bind(temp);
        }
        if let Some(active) = updates.is_active {
            q = q.bind(active);
        }
        q = q.bind(id);
        q.execute(&self.pool).await?;

        self.get_prompt(id).await
    }

    pub async fn delete_prompt(&self, id: &str) -> Result<bool, sqlx::Error> {
        // Don't delete builtin prompts
        let result = sqlx::query("DELETE FROM prompt_library WHERE id = ? AND is_builtin = 0")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn duplicate_prompt(
        &self,
        id: &str,
        new_name: &str,
    ) -> Result<Option<Prompt>, sqlx::Error> {
        if let Some(original) = self.get_prompt(id).await? {
            let input = PromptCreate {
                name: new_name.to_string(),
                description: original.description,
                category: original.category,
                system_prompt: original.system_prompt,
                user_prompt_template: original.user_prompt_template,
                model_id: original.model_id,
                temperature: Some(original.temperature),
                max_tokens: original.max_tokens,
            };
            Ok(Some(self.create_prompt(input).await?))
        } else {
            Ok(None)
        }
    }

    // ============================================
    // Model Configuration Operations
    // ============================================

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfig>, sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id, name, display_name, model_type, base_url, capabilities, 
                   default_temperature, default_max_tokens, is_available, last_health_check, created_at
            FROM model_configurations ORDER BY model_type, name
        "#)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_model_config(r)).collect())
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfig>, sqlx::Error> {
        let row = sqlx::query(r#"
            SELECT id, name, display_name, model_type, base_url, capabilities, 
                   default_temperature, default_max_tokens, is_available, last_health_check, created_at
            FROM model_configurations WHERE id = ?
        "#)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| self.row_to_model_config(&r)))
    }

    pub async fn get_model_config_by_name(
        &self,
        name: &str,
    ) -> Result<Option<ModelConfig>, sqlx::Error> {
        let row = sqlx::query(r#"
            SELECT id, name, display_name, model_type, base_url, capabilities, 
                   default_temperature, default_max_tokens, is_available, last_health_check, created_at
            FROM model_configurations WHERE name = ?
        "#)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| self.row_to_model_config(&r)))
    }

    pub async fn update_model_availability(
        &self,
        name: &str,
        is_available: bool,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE model_configurations SET is_available = ?, last_health_check = ? WHERE name = ?")
            .bind(is_available)
            .bind(&now)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_model_config(
        &self,
        input: ModelConfigCreate,
    ) -> Result<ModelConfig, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let caps = input.capabilities.clone().unwrap_or_default();
        let caps_json = serde_json::to_string(&caps).unwrap_or_default();

        sqlx::query(r#"
            INSERT INTO model_configurations 
            (id, name, display_name, model_type, base_url, capabilities, default_temperature, default_max_tokens, is_available, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)
        "#)
        .bind(&id)
        .bind(&input.name)
        .bind(&input.display_name)
        .bind(&input.model_type)
        .bind(input.base_url.as_deref().unwrap_or("http://localhost:8080"))
        .bind(&caps_json)
        .bind(input.default_temperature.unwrap_or(0.5))
        .bind(input.default_max_tokens.unwrap_or(2048))
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        Ok(ModelConfig {
            id,
            name: input.name,
            display_name: input.display_name,
            model_type: input.model_type,
            base_url: input
                .base_url
                .unwrap_or_else(|| "http://localhost:8080".to_string()),
            capabilities: input.capabilities.unwrap_or_default(),
            default_temperature: input.default_temperature.unwrap_or(0.5),
            default_max_tokens: input.default_max_tokens.unwrap_or(2048),
            is_available: false,
            last_health_check: None,
            created_at: now,
        })
    }

    // ============================================
    // Use Case Operations
    // ============================================

    pub async fn list_use_cases(&self) -> Result<Vec<UseCase>, sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id, use_case, display_name, description, prompt_id, model_id, priority, conditions, is_active, created_at
            FROM use_case_mappings ORDER BY priority DESC, use_case
        "#)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_use_case(r)).collect())
    }

    pub async fn get_use_case(&self, use_case: &str) -> Result<Option<UseCase>, sqlx::Error> {
        let row = sqlx::query(r#"
            SELECT id, use_case, display_name, description, prompt_id, model_id, priority, conditions, is_active, created_at
            FROM use_case_mappings WHERE use_case = ?
        "#)
        .bind(use_case)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| self.row_to_use_case(&r)))
    }

    pub async fn update_use_case_mapping(
        &self,
        use_case: &str,
        prompt_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Result<Option<UseCase>, sqlx::Error> {
        sqlx::query("UPDATE use_case_mappings SET prompt_id = ?, model_id = ? WHERE use_case = ?")
            .bind(prompt_id)
            .bind(model_id)
            .bind(use_case)
            .execute(&self.pool)
            .await?;

        self.get_use_case(use_case).await
    }

    pub async fn get_resolved_use_case(
        &self,
        use_case: &str,
    ) -> Result<Option<ResolvedUseCase>, sqlx::Error> {
        if let Some(uc) = self.get_use_case(use_case).await? {
            let prompt = if let Some(ref pid) = uc.prompt_id {
                self.get_prompt(pid).await?
            } else {
                None
            };
            let model = if let Some(ref mid) = uc.model_id {
                self.get_model_config(mid).await?
            } else {
                None
            };
            Ok(Some(ResolvedUseCase {
                use_case: uc,
                prompt,
                model,
            }))
        } else {
            Ok(None)
        }
    }

    // ============================================
    // Export/Import
    // ============================================

    pub async fn export_prompts(&self) -> Result<String, sqlx::Error> {
        let prompts = self.list_prompts(None).await?;
        let export = serde_json::json!({
            "version": "1.0",
            "exported_at": Utc::now().to_rfc3339(),
            "prompts": prompts.into_iter().filter(|p| !p.is_builtin).collect::<Vec<_>>()
        });
        Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
    }

    pub async fn import_prompts(&self, json: &str) -> Result<Vec<Prompt>, sqlx::Error> {
        let data: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid JSON: {}", e)))?;

        let mut imported = Vec::new();
        if let Some(prompts) = data.get("prompts").and_then(|p| p.as_array()) {
            for p in prompts {
                if let Ok(input) = serde_json::from_value::<PromptCreate>(p.clone()) {
                    if let Ok(prompt) = self.create_prompt(input).await {
                        imported.push(prompt);
                    }
                }
            }
        }
        Ok(imported)
    }

    // ============================================
    // Theme-Specific Prompt Methods (Phase 2)
    // ============================================

    /// List prompts for a specific theme
    pub async fn list_prompts_by_theme(&self, theme: &str) -> Result<Vec<Prompt>, sqlx::Error> {
        let rows = sqlx::query(r#"
            SELECT id, name, description, category, system_prompt, user_prompt_template, 
                   model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
            FROM prompt_library WHERE theme = ? ORDER BY name, version DESC
        "#)
        .bind(theme)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_prompt(r)).collect())
    }

    /// Get the latest version of a prompt by name and theme
    pub async fn get_latest_prompt(
        &self,
        name: &str,
        theme: Option<&str>,
    ) -> Result<Option<Prompt>, sqlx::Error> {
        let row = if let Some(t) = theme {
            sqlx::query(r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library WHERE name = ? AND theme = ? ORDER BY version DESC LIMIT 1
            "#)
            .bind(name)
            .bind(t)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library WHERE name = ? AND theme IS NULL ORDER BY version DESC LIMIT 1
            "#)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
        };

        Ok(row.map(|r| self.row_to_prompt(&r)))
    }

    /// Get all versions of a prompt by name
    pub async fn get_prompt_versions(
        &self,
        name: &str,
        theme: Option<&str>,
    ) -> Result<Vec<Prompt>, sqlx::Error> {
        let rows = if let Some(t) = theme {
            sqlx::query(r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library WHERE name = ? AND theme = ? ORDER BY version DESC
            "#)
            .bind(name)
            .bind(t)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(r#"
                SELECT id, name, description, category, system_prompt, user_prompt_template, 
                       model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at
                FROM prompt_library WHERE name = ? AND theme IS NULL ORDER BY version DESC
            "#)
            .bind(name)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.iter().map(|r| self.row_to_prompt(r)).collect())
    }

    /// Create a new version of an existing prompt (version control)
    pub async fn create_prompt_version(
        &self,
        prompt_id: &str,
        updates: PromptUpdate,
    ) -> Result<Prompt, sqlx::Error> {
        // Get the current prompt
        let current = self
            .get_prompt(prompt_id)
            .await?
            .ok_or_else(|| sqlx::Error::Protocol("Prompt not found".to_string()))?;

        // Create new version
        let new_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let new_version = current.version + 1;

        let new_name = updates.name.as_ref().unwrap_or(&current.name);
        let new_desc = updates
            .description
            .as_ref()
            .or(current.description.as_ref());
        let new_cat = updates.category.as_ref().unwrap_or(&current.category);
        let new_prompt = updates
            .system_prompt
            .as_ref()
            .unwrap_or(&current.system_prompt);
        let new_user_template = updates
            .user_prompt_template
            .as_ref()
            .or(current.user_prompt_template.as_ref());
        let new_model_id = updates.model_id.as_ref().or(current.model_id.as_ref());
        let new_temp = updates.temperature.unwrap_or(current.temperature);

        sqlx::query(r#"
            INSERT INTO prompt_library 
            (id, name, description, category, system_prompt, user_prompt_template, model_id, temperature, max_tokens, theme, version, is_builtin, is_active, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
        "#)
        .bind(&new_id)
        .bind(new_name)
        .bind(new_desc)
        .bind(new_cat)
        .bind(new_prompt)
        .bind(new_user_template)
        .bind(new_model_id)
        .bind(new_temp)
        .bind(&current.max_tokens)
        .bind(&current.theme)
        .bind(new_version)
        .bind(current.is_builtin)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        Ok(Prompt {
            id: new_id,
            name: new_name.clone(),
            description: new_desc.cloned(),
            category: new_cat.clone(),
            system_prompt: new_prompt.clone(),
            user_prompt_template: new_user_template.cloned(),
            model_id: new_model_id.cloned(),
            temperature: new_temp,
            max_tokens: current.max_tokens,
            theme: current.theme.clone(),
            version: new_version,
            is_builtin: current.is_builtin,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    /// Create a theme-specific prompt
    pub async fn create_theme_prompt(
        &self,
        theme: &str,
        name: &str,
        description: &str,
        category: &str,
        system_prompt: &str,
        model_id: Option<&str>,
        temperature: Option<f32>,
    ) -> Result<Prompt, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(r#"
            INSERT INTO prompt_library 
            (id, name, description, category, system_prompt, model_id, temperature, theme, version, is_builtin, is_active, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 1, 1, ?, ?)
        "#)
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(category)
        .bind(system_prompt)
        .bind(model_id)
        .bind(temperature.unwrap_or(0.5))
        .bind(theme)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.pool)
        .await?;

        Ok(Prompt {
            id,
            name: name.to_string(),
            description: Some(description.to_string()),
            category: category.to_string(),
            system_prompt: system_prompt.to_string(),
            user_prompt_template: None,
            model_id: model_id.map(|s| s.to_string()),
            temperature: temperature.unwrap_or(0.5),
            max_tokens: None,
            theme: Some(theme.to_string()),
            version: 1,
            is_builtin: true,
            is_active: true,
            created_at: now,
            updated_at: now,
        })
    }

    // ============================================
    // Helper Methods
    // ============================================

    fn row_to_prompt(&self, row: &sqlx::sqlite::SqliteRow) -> Prompt {
        Prompt {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            category: row.get("category"),
            system_prompt: row.get("system_prompt"),
            user_prompt_template: row.get("user_prompt_template"),
            model_id: row.get("model_id"),
            temperature: row.get("temperature"),
            max_tokens: row.get("max_tokens"),
            theme: row.get("theme"),
            version: row.get("version"),
            is_builtin: row.get("is_builtin"),
            is_active: row.get("is_active"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }

    fn row_to_model_config(&self, row: &sqlx::sqlite::SqliteRow) -> ModelConfig {
        let caps_str: String = row.get("capabilities");
        let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();

        ModelConfig {
            id: row.get("id"),
            name: row.get("name"),
            display_name: row.get("display_name"),
            model_type: row.get("model_type"),
            base_url: row.get("base_url"),
            capabilities,
            default_temperature: row.get("default_temperature"),
            default_max_tokens: row.get("default_max_tokens"),
            is_available: row.get("is_available"),
            last_health_check: row
                .get::<Option<String>, _>("last_health_check")
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }

    fn row_to_use_case(&self, row: &sqlx::sqlite::SqliteRow) -> UseCase {
        UseCase {
            id: row.get("id"),
            use_case: row.get("use_case"),
            display_name: row.get("display_name"),
            description: row.get("description"),
            prompt_id: row.get("prompt_id"),
            model_id: row.get("model_id"),
            priority: row.get("priority"),
            conditions: row.get("conditions"),
            is_active: row.get("is_active"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}
