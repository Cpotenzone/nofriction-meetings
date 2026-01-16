//! Supabase Client for syncing activity data to Postgres
//!
//! Handles structured data storage and time-based queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::sync::Arc;
use parking_lot::RwLock;

/// Activity record for Supabase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub category: String,
    pub summary: String,
    pub focus_area: Option<String>,
    pub pinecone_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Daily summary record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: String,
    pub total_hours: f64,
    pub categories: serde_json::Value,
    pub top_activities: serde_json::Value,
}

/// Supabase Postgres client
pub struct SupabaseClient {
    pool: Arc<RwLock<Option<PgPool>>>,
    connection_string: Arc<RwLock<Option<String>>>,
}

impl SupabaseClient {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(RwLock::new(None)),
            connection_string: Arc::new(RwLock::new(None)),
        }
    }

    /// Set connection string
    pub fn set_connection_string(&self, conn_str: String) {
        *self.connection_string.write() = Some(conn_str);
    }

    /// Connect to Supabase Postgres
    pub async fn connect(&self) -> Result<(), String> {
        let conn_str = self.connection_string.read().clone()
            .ok_or("No connection string configured")?;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&conn_str)
            .await
            .map_err(|e| format!("Failed to connect to Supabase: {}", e))?;

        // Run migrations
        self.run_migrations(&pool).await?;

        *self.pool.write() = Some(pool);
        log::info!("✅ Connected to Supabase Postgres");
        Ok(())
    }

    /// Run schema migrations
    async fn run_migrations(&self, pool: &PgPool) -> Result<(), String> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS activities (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                start_time TIMESTAMPTZ NOT NULL,
                end_time TIMESTAMPTZ,
                duration_seconds INTEGER,
                app_name TEXT,
                window_title TEXT,
                category TEXT NOT NULL DEFAULT 'other',
                summary TEXT NOT NULL,
                focus_area TEXT,
                pinecone_id TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
        "#)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create activities table: {}", e))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS daily_summaries (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                date DATE NOT NULL UNIQUE,
                total_hours DOUBLE PRECISION,
                categories JSONB,
                top_activities JSONB,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
        "#)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create daily_summaries table: {}", e))?;

        // Create indexes
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activities_start_time ON activities(start_time)")
            .execute(pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activities_category ON activities(category)")
            .execute(pool).await;

        log::info!("Supabase schema migrations completed");
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.pool.read().is_some()
    }

    /// Insert an activity
    pub async fn insert_activity(&self, activity: &Activity) -> Result<String, String> {
        let pool = self.pool.read().clone()
            .ok_or("Not connected to Supabase")?;

        let row = sqlx::query(r#"
            INSERT INTO activities (start_time, end_time, duration_seconds, app_name, window_title, category, summary, focus_area, pinecone_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id::text
        "#)
        .bind(&activity.start_time)
        .bind(&activity.end_time)
        .bind(&activity.duration_seconds)
        .bind(&activity.app_name)
        .bind(&activity.window_title)
        .bind(&activity.category)
        .bind(&activity.summary)
        .bind(&activity.focus_area)
        .bind(&activity.pinecone_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| format!("Failed to insert activity: {}", e))?;

        let id: String = row.get("id");
        log::info!("💾 Activity synced to Supabase: {}", id);
        Ok(id)
    }

    /// Batch insert activities
    pub async fn insert_activities(&self, activities: &[Activity]) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for activity in activities {
            match self.insert_activity(activity).await {
                Ok(id) => ids.push(id),
                Err(e) => log::error!("Failed to insert activity: {}", e),
            }
        }
        Ok(ids)
    }

    /// Query activities by time range
    pub async fn query_activities(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Activity>, String> {
        let pool = self.pool.read().clone()
            .ok_or("Not connected to Supabase")?;

        let rows = sqlx::query(r#"
            SELECT id::text, start_time, end_time, duration_seconds, 
                   app_name, window_title, category, summary, focus_area, pinecone_id, created_at
            FROM activities
            WHERE start_time >= $1 AND start_time <= $2
            ORDER BY start_time ASC
        "#)
        .bind(&start)
        .bind(&end)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("Failed to query activities: {}", e))?;

        Ok(rows.into_iter().map(|r| Activity {
            id: Some(r.get("id")),
            start_time: r.get("start_time"),
            end_time: r.get("end_time"),
            duration_seconds: r.get("duration_seconds"),
            app_name: r.get("app_name"),
            window_title: r.get("window_title"),
            category: r.get("category"),
            summary: r.get("summary"),
            focus_area: r.get("focus_area"),
            pinecone_id: r.get("pinecone_id"),
            created_at: r.get("created_at"),
        }).collect())
    }

    /// Get time spent by category for a date
    pub async fn get_time_by_category(&self, date: &str) -> Result<serde_json::Value, String> {
        let pool = self.pool.read().clone()
            .ok_or("Not connected to Supabase")?;

        let rows = sqlx::query(r#"
            SELECT category, SUM(duration_seconds) as total_seconds
            FROM activities
            WHERE DATE(start_time) = $1::date
            GROUP BY category
            ORDER BY total_seconds DESC
        "#)
        .bind(date)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("Failed to query time by category: {}", e))?;

        let mut result = serde_json::Map::new();
        for row in rows {
            let category: String = row.get("category");
            let seconds: i64 = row.get("total_seconds");
            result.insert(category, serde_json::json!(seconds));
        }

        Ok(serde_json::Value::Object(result))
    }

    /// Upsert daily summary
    pub async fn upsert_daily_summary(&self, summary: &DailySummary) -> Result<(), String> {
        let pool = self.pool.read().clone()
            .ok_or("Not connected to Supabase")?;

        sqlx::query(r#"
            INSERT INTO daily_summaries (date, total_hours, categories, top_activities)
            VALUES ($1::date, $2, $3, $4)
            ON CONFLICT (date) DO UPDATE SET
                total_hours = $2,
                categories = $3,
                top_activities = $4
        "#)
        .bind(&summary.date)
        .bind(&summary.total_hours)
        .bind(&summary.categories)
        .bind(&summary.top_activities)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to upsert daily summary: {}", e))?;

        Ok(())
    }
}

impl Default for SupabaseClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SupabaseClient {
    /// Get a clone of the pool (for async operations without holding guard)
    pub fn get_pool(&self) -> Option<PgPool> {
        self.pool.read().clone()
    }
    
    /// Set the pool (for standalone connect)
    pub fn set_pool(&self, pool: PgPool) {
        *self.pool.write() = Some(pool);
    }
    
    /// Get connection string (for standalone connect)
    pub fn get_connection_string(&self) -> Option<String> {
        self.connection_string.read().clone()
    }
}

// ============================================
// Standalone async functions (avoid RwLock guard issues)
// ============================================

/// Insert activity to Supabase with provided pool (no guard held)
pub async fn supabase_insert_activity(
    pool: &PgPool,
    activity: &Activity,
) -> Result<String, String> {
    let row = sqlx::query(r#"
        INSERT INTO activities (start_time, end_time, duration_seconds, app_name, window_title, category, summary, focus_area, pinecone_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id::text
    "#)
    .bind(&activity.start_time)
    .bind(&activity.end_time)
    .bind(&activity.duration_seconds)
    .bind(&activity.app_name)
    .bind(&activity.window_title)
    .bind(&activity.category)
    .bind(&activity.summary)
    .bind(&activity.focus_area)
    .bind(&activity.pinecone_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert activity: {}", e))?;

    let id: String = row.get("id");
    log::info!("💾 Activity synced to Supabase: {}", id);
    Ok(id)
}

/// Query activities by time range (standalone)
pub async fn supabase_query_activities(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Activity>, String> {
    let rows = sqlx::query(r#"
        SELECT id::text, start_time, end_time, duration_seconds, 
               app_name, window_title, category, summary, focus_area, pinecone_id, created_at
        FROM activities
        WHERE start_time >= $1 AND start_time <= $2
        ORDER BY start_time ASC
    "#)
    .bind(&start)
    .bind(&end)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query activities: {}", e))?;

    Ok(rows.into_iter().map(|r| Activity {
        id: Some(r.get("id")),
        start_time: r.get("start_time"),
        end_time: r.get("end_time"),
        duration_seconds: r.get("duration_seconds"),
        app_name: r.get("app_name"),
        window_title: r.get("window_title"),
        category: r.get("category"),
        summary: r.get("summary"),
        focus_area: r.get("focus_area"),
        pinecone_id: r.get("pinecone_id"),
        created_at: r.get("created_at"),
    }).collect())
}

/// Connect to Supabase and return pool (standalone - avoids guard across await)
pub async fn supabase_connect_pool(conn_str: &str) -> Result<PgPool, String> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(conn_str)
        .await
        .map_err(|e| format!("Failed to connect to Supabase: {}", e))?;

    // Run migrations
    supabase_run_migrations(&pool).await?;
    
    log::info!("✅ Connected to Supabase Postgres");
    Ok(pool)
}

/// Run schema migrations (standalone)
async fn supabase_run_migrations(pool: &PgPool) -> Result<(), String> {
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS activities (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            start_time TIMESTAMPTZ NOT NULL,
            end_time TIMESTAMPTZ,
            duration_seconds INTEGER,
            app_name TEXT,
            window_title TEXT,
            category TEXT NOT NULL DEFAULT 'other',
            summary TEXT NOT NULL,
            focus_area TEXT,
            pinecone_id TEXT,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
    "#)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create activities table: {}", e))?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS daily_summaries (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            date DATE NOT NULL UNIQUE,
            total_hours DOUBLE PRECISION,
            categories JSONB,
            top_activities JSONB,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
    "#)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create daily_summaries table: {}", e))?;

    // Create indexes
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activities_start_time ON activities(start_time)")
        .execute(pool).await;
    let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_activities_category ON activities(category)")
        .execute(pool).await;

    log::info!("Supabase schema migrations completed");
    Ok(())
}


