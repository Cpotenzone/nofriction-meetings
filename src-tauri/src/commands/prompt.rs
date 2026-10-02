// noFriction Meetings - Prompt & Model Commands
// Prompt CRUD, model configuration, and use case mapping

use crate::AppState;
use tauri::State;

// Prompt Management Commands
// ============================================

/// List all prompts, optionally filtered by category
#[tauri::command(rename_all = "camelCase")]
pub async fn list_prompts(
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .list_prompts(category.as_deref())
        .await
        .map_err(|e| format!("Failed to list prompts: {}", e))
}

/// Get a single prompt by ID
#[tauri::command(rename_all = "camelCase")]
pub async fn get_prompt(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .get_prompt(&id)
        .await
        .map_err(|e| format!("Failed to get prompt: {}", e))
}

/// Create a new prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_prompt(
    input: crate::prompt_manager::PromptCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::Prompt, String> {
    state
        .prompt_manager
        .create_prompt(input)
        .await
        .map_err(|e| format!("Failed to create prompt: {}", e))
}

/// Update an existing prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn update_prompt(
    id: String,
    updates: crate::prompt_manager::PromptUpdate,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .update_prompt(&id, updates)
        .await
        .map_err(|e| format!("Failed to update prompt: {}", e))
}

/// Delete a prompt (only non-builtin prompts can be deleted)
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_prompt(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    state
        .prompt_manager
        .delete_prompt(&id)
        .await
        .map_err(|e| format!("Failed to delete prompt: {}", e))
}

/// Duplicate a prompt with a new name
#[tauri::command(rename_all = "camelCase")]
pub async fn duplicate_prompt(
    id: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .duplicate_prompt(&id, &new_name)
        .await
        .map_err(|e| format!("Failed to duplicate prompt: {}", e))
}

/// Export all custom prompts as JSON
#[tauri::command(rename_all = "camelCase")]
pub async fn export_prompts(state: State<'_, AppState>) -> Result<String, String> {
    state
        .prompt_manager
        .export_prompts()
        .await
        .map_err(|e| format!("Failed to export prompts: {}", e))
}

/// Import prompts from JSON
#[tauri::command(rename_all = "camelCase")]
pub async fn import_prompts(
    json: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::Prompt>, String> {
    state
        .prompt_manager
        .import_prompts(&json)
        .await
        .map_err(|e| format!("Failed to import prompts: {}", e))
}

// ============================================
// Model Configuration Commands
// ============================================

/// List all model configurations
#[tauri::command(rename_all = "camelCase")]
pub async fn list_model_configs(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to list model configs: {}", e))
}

/// Get a model config by ID
#[tauri::command(rename_all = "camelCase")]
pub async fn get_model_config(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ModelConfig>, String> {
    state
        .prompt_manager
        .get_model_config(&id)
        .await
        .map_err(|e| format!("Failed to get model config: {}", e))
}

/// Create a new model configuration
#[tauri::command(rename_all = "camelCase")]
pub async fn create_model_config(
    input: crate::prompt_manager::ModelConfigCreate,
    state: State<'_, AppState>,
) -> Result<crate::prompt_manager::ModelConfig, String> {
    state
        .prompt_manager
        .create_model_config(input)
        .await
        .map_err(|e| format!("Failed to create model config: {}", e))
}

/// Model ids offered by the active text provider (fresh list).
async fn active_provider_model_ids() -> Result<Vec<String>, String> {
    let provider = crate::ai::config::snapshot()
        .text
        .map(|s| s.provider)
        .ok_or_else(|| crate::ai::AiError::NoProvider.to_string())?;
    let models = crate::ai::commands::ai_list_models(provider).await?;
    Ok(models.into_iter().map(|m| m.id).collect())
}

/// Refresh model availability against the active AI provider's model list
#[tauri::command(rename_all = "camelCase")]
pub async fn refresh_model_availability(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::ModelConfig>, String> {
    let model_names = active_provider_model_ids().await?;

    // Update availability for each configured model
    let configs = state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to list configs: {}", e))?;

    for config in &configs {
        let is_available = model_names.iter().any(|n| n.starts_with(&config.name));
        let _ = state
            .prompt_manager
            .update_model_availability(&config.name, is_available)
            .await;
    }

    // Return updated list
    state
        .prompt_manager
        .list_model_configs()
        .await
        .map_err(|e| format!("Failed to refresh model configs: {}", e))
}

/// List models offered by the active AI provider (legacy name; returns
/// `[{name}]` objects like the old Ollama tag list).
#[tauri::command(rename_all = "camelCase")]
pub async fn list_ollama_models(
    _state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(active_provider_model_ids()
        .await?
        .into_iter()
        .map(|name| serde_json::json!({ "name": name }))
        .collect())
}

// ============================================
// Use Case Mapping Commands
// ============================================

/// List all use case mappings
#[tauri::command(rename_all = "camelCase")]
pub async fn list_use_cases(
    state: State<'_, AppState>,
) -> Result<Vec<crate::prompt_manager::UseCase>, String> {
    state
        .prompt_manager
        .list_use_cases()
        .await
        .map_err(|e| format!("Failed to list use cases: {}", e))
}

/// Get a specific use case with resolved prompt and model
#[tauri::command(rename_all = "camelCase")]
pub async fn get_resolved_use_case(
    use_case: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::ResolvedUseCase>, String> {
    state
        .prompt_manager
        .get_resolved_use_case(&use_case)
        .await
        .map_err(|e| format!("Failed to get use case: {}", e))
}

/// Update use case mapping
#[tauri::command(rename_all = "camelCase")]
pub async fn update_use_case_mapping(
    use_case: String,
    prompt_id: Option<String>,
    model_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<crate::prompt_manager::UseCase>, String> {
    state
        .prompt_manager
        .update_use_case_mapping(&use_case, prompt_id.as_deref(), model_id.as_deref())
        .await
        .map_err(|e| format!("Failed to update use case: {}", e))
}

/// Test a prompt with sample input
#[tauri::command(rename_all = "camelCase")]
pub async fn test_prompt(
    prompt_id: String,
    test_input: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get the prompt
    let prompt = state
        .prompt_manager
        .get_prompt(&prompt_id)
        .await
        .map_err(|e| format!("Failed to get prompt: {}", e))?
        .ok_or_else(|| "Prompt not found".to_string())?;

    // Runs on the active text provider (Settings → AI Engine)
    crate::ai_client::AIClient::new()
        .complete_with(
            Some(&prompt.system_prompt),
            &test_input,
            crate::ai_client::DEFAULT_MAX_TOKENS,
            prompt.temperature,
        )
        .await
}

// ============================================================================
// Activity Theme Commands
// ============================================

#[tauri::command(rename_all = "camelCase")]
pub async fn set_active_theme(theme: String, state: State<'_, AppState>) -> Result<(), String> {
    // Validate theme name
    let valid_themes = [
        "prospecting",
        "fundraising",
        "product_dev",
        "admin",
        "personal",
    ];
    if !valid_themes.contains(&theme.as_str()) {
        return Err(format!(
            "Invalid theme: {}. Must be one of: {:?}",
            theme, valid_themes
        ));
    }

    // End any open theme session first
    if let Ok(Some(session_id)) = state.database.get_last_open_session().await {
        let _ = state.database.end_theme_session(session_id).await; // Best effort
    }

    // Save to settings
    state
        .settings
        .set_active_theme(&theme)
        .await
        .map_err(|e| format!("Failed to set theme: {}", e))?;

    // Apply theme-specific settings
    let interval_ms = state
        .settings
        .get_theme_interval(&theme)
        .await
        .map_err(|e| format!("Failed to get theme interval: {}", e))?;

    // Update capture engine interval (release lock immediately)
    {
        let engine = state.capture_engine.write();
        engine.set_frame_interval(interval_ms);
    } // Lock dropped here

    // Auto-enable mic transcription for prospecting/fundraising (meeting-heavy themes)
    let enable_mic = matches!(theme.as_str(), "prospecting" | "fundraising");
    state
        .settings
        .set_capture_microphone(enable_mic)
        .await
        .map_err(|e| format!("Failed to set mic capture: {}", e))?;

    // Start new theme session
    state
        .database
        .start_theme_session(&theme)
        .await
        .map_err(|e| format!("Failed to start theme session: {}", e))?;

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_recent_entities(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(50);
    state
        .database
        .get_recent_entities(limit as i32)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_active_theme(state: State<'_, AppState>) -> Result<String, String> {
    state
        .settings
        .get_active_theme()
        .await
        .map_err(|e| format!("Failed to get active theme: {}", e))
}

#[derive(serde::Serialize)]
pub struct ThemeSettings {
    active_theme: String,
    prospecting_interval_ms: u32,
    fundraising_interval_ms: u32,
    product_dev_interval_ms: u32,
    admin_interval_ms: u32,
    personal_interval_ms: u32,
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_theme_settings(state: State<'_, AppState>) -> Result<ThemeSettings, String> {
    let settings = state
        .settings
        .get_all()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))?;

    Ok(ThemeSettings {
        active_theme: settings.active_theme,
        prospecting_interval_ms: settings.prospecting_interval_ms,
        fundraising_interval_ms: settings.fundraising_interval_ms,
        product_dev_interval_ms: settings.product_dev_interval_ms,
        admin_interval_ms: settings.admin_interval_ms,
        personal_interval_ms: settings.personal_interval_ms,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_theme_interval(
    theme: String,
    interval_ms: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Save to settings
    state
        .settings
        .set_theme_interval(&theme, interval_ms)
        .await
        .map_err(|e| format!("Failed to set interval: {}", e))?;

    // If this is the active theme, update capture engine immediately
    let active_theme = state
        .settings
        .get_active_theme()
        .await
        .map_err(|e| format!("Failed to get active theme: {}", e))?;

    if active_theme == theme {
        let engine = state.capture_engine.write();
        engine.set_frame_interval(interval_ms);
    }

    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_theme_time_today(
    theme: String,
    state: State<'_, AppState>,
) -> Result<f64, String> {
    let seconds = state
        .database
        .get_theme_time_today(&theme)
        .await
        .map_err(|e| format!("Failed to get theme time: {}", e))?;

    // Convert to hours with 1 decimal place
    Ok((seconds as f64) / 3600.0)
}
// ============================================
// Phase 2: Theme-Specific Prompt Management Commands
// ============================================

use crate::prompt_manager::{Prompt, PromptUpdate};

/// List all prompts for a specific theme
#[tauri::command(rename_all = "camelCase")]
pub async fn list_prompts_by_theme(
    theme: String,
    state: State<'_, AppState>,
) -> Result<Vec<Prompt>, String> {
    state
        .prompt_manager
        .list_prompts_by_theme(&theme)
        .await
        .map_err(|e| format!("Failed to list prompts by theme: {}", e))
}

/// Get the latest version of a prompt by name and optional theme
#[tauri::command(rename_all = "camelCase")]
pub async fn get_latest_prompt(
    name: String,
    theme: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<Prompt>, String> {
    state
        .prompt_manager
        .get_latest_prompt(&name, theme.as_deref())
        .await
        .map_err(|e| format!("Failed to get latest prompt: {}", e))
}

/// Get all versions of a prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn get_prompt_versions(
    name: String,
    theme: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Prompt>, String> {
    state
        .prompt_manager
        .get_prompt_versions(&name, theme.as_deref())
        .await
        .map_err(|e| format!("Failed to get prompt versions: {}", e))
}

/// Create a new version of an existing prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_prompt_version(
    prompt_id: String,
    updates: PromptUpdate,
    state: State<'_, AppState>,
) -> Result<Prompt, String> {
    state
        .prompt_manager
        .create_prompt_version(&prompt_id, updates)
        .await
        .map_err(|e| format!("Failed to create prompt version: {}", e))
}

/// Create a theme-specific prompt
#[tauri::command(rename_all = "camelCase")]
pub async fn create_theme_prompt(
    theme: String,
    name: String,
    description: String,
    category: String,
    system_prompt: String,
    model_id: Option<String>,
    temperature: Option<f32>,
    state: State<'_, AppState>,
) -> Result<Prompt, String> {
    state
        .prompt_manager
        .create_theme_prompt(
            &theme,
            &name,
            &description,
            &category,
            &system_prompt,
            model_id.as_deref(),
            temperature,
        )
        .await
        .map_err(|e| format!("Failed to create theme prompt: {}", e))
}
// ============================================================================
