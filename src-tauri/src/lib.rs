// noFriction Meetings - Main Library
// Professional macOS meeting transcription app

pub mod capture_engine;
pub mod deepgram_client;
pub mod database;
pub mod settings;
pub mod commands;
pub mod ai_client;
pub mod vlm_client;
pub mod supabase_client;
pub mod pinecone_client;
pub mod prompt_manager;
pub mod menu_builder;

use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{Manager, AppHandle};

use capture_engine::CaptureEngine;
use deepgram_client::DeepgramClient;
use database::DatabaseManager;
use settings::SettingsManager;
use vlm_client::VLMClient;
use supabase_client::SupabaseClient;
use pinecone_client::PineconeClient;
use prompt_manager::PromptManager;

/// Application state shared across commands
pub struct AppState {
    pub capture_engine: Arc<RwLock<CaptureEngine>>,
    pub deepgram_client: Arc<RwLock<DeepgramClient>>,
    pub database: Arc<DatabaseManager>,
    pub settings: Arc<SettingsManager>,
    pub vlm_client: Arc<RwLock<VLMClient>>,
    pub supabase_client: Arc<RwLock<SupabaseClient>>,
    pub pinecone_client: Arc<RwLock<PineconeClient>>,
    pub prompt_manager: Arc<PromptManager>,
}

impl AppState {
    pub async fn new(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        // Get app data directory for SQLite database
        let app_data_dir = app.path().app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        
        std::fs::create_dir_all(&app_data_dir)?;
        
        let db_path = app_data_dir.join("nofriction_meetings.db");
        
        // Initialize database
        let database = DatabaseManager::new(&db_path).await?;
        database.run_migrations().await?;
        
        // Initialize settings manager (uses same pool)
        let settings = SettingsManager::new(database.get_pool());
        settings.init().await?;
        
        // Load saved settings
        let saved_settings = settings.get_all().await.unwrap_or_default();
        
        // Initialize Deepgram client with saved API key
        let deepgram = DeepgramClient::new();
        if let Some(ref api_key) = saved_settings.deepgram_api_key {
            deepgram.set_api_key(api_key.clone());
            log::info!("Loaded saved Deepgram API key");
        }
        
        // Initialize capture engine with saved preferences
        let capture = CaptureEngine::new();
        if let Some(ref mic) = saved_settings.selected_microphone {
            capture.set_microphone(mic.clone());
            log::info!("Loaded saved microphone: {}", mic);
        }
        if let Some(monitor) = saved_settings.selected_monitor {
            capture.set_monitor(monitor);
            log::info!("Loaded saved monitor: {}", monitor);
        }
        
        // Initialize knowledge base clients
        let vlm = VLMClient::new();
        let supabase = SupabaseClient::new();
        let pinecone = PineconeClient::new();
        
        // Initialize prompt manager with same pool
        let prompt_manager = PromptManager::new((*database.get_pool()).clone());
        prompt_manager.run_migrations().await?;
        log::info!("Prompt manager initialized with default presets");
        
        log::info!("Database initialized at: {:?}", db_path);
        
        Ok(Self {
            capture_engine: Arc::new(RwLock::new(capture)),
            deepgram_client: Arc::new(RwLock::new(deepgram)),
            database: Arc::new(database),
            settings: Arc::new(settings),
            vlm_client: Arc::new(RwLock::new(vlm)),
            supabase_client: Arc::new(RwLock::new(supabase)),
            pinecone_client: Arc::new(RwLock::new(pinecone)),
            prompt_manager: Arc::new(prompt_manager),
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();
            
            // Initialize app state asynchronously
            tauri::async_runtime::spawn(async move {
                match AppState::new(&handle).await {
                    Ok(state) => {
                        handle.manage(state);
                        log::info!("noFriction Meetings v1.0.0 initialized successfully");
                    }
                    Err(e) => {
                        log::error!("Failed to initialize app state: {}", e);
                    }
                }
            });
            
            // Create native menu bar
            let menu = menu_builder::create_menu(app.handle())?;
            app.set_menu(menu)?;
            log::info!("Native macOS menu bar created");
            
            Ok(())
        })
        .on_menu_event(|app, event| {
            menu_builder::handle_menu_event(app, event.id().as_ref());
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_recording,
            commands::stop_recording,
            commands::get_recording_status,
            commands::capture_screenshot,
            commands::get_transcripts,
            commands::search_transcripts,
            commands::get_frames,
            commands::get_frame_count,
            commands::get_frame_thumbnail,
            commands::get_synced_timeline,
            commands::get_audio_devices,
            commands::set_audio_device,
            commands::get_monitors,
            commands::set_monitor,
            commands::set_deepgram_api_key,
            commands::get_deepgram_api_key,
            commands::get_meetings,
            commands::get_meeting,
            commands::delete_meeting,
            commands::get_settings,
            // AI Commands
            commands::check_ollama,
            commands::get_ollama_models,
            commands::get_ai_presets,
            commands::ai_chat,
            commands::summarize_meeting,
            commands::extract_action_items,
            // Knowledge Base Commands
            commands::check_vlm,
            commands::check_vlm_vision,
            commands::analyze_frame,
            commands::analyze_frames_batch,
            commands::configure_supabase,
            commands::check_supabase,
            commands::sync_activity_to_supabase,
            commands::query_activities,
            commands::configure_pinecone,
            commands::check_pinecone,
            commands::upsert_to_pinecone,
            commands::semantic_search,
            commands::get_pinecone_stats,
            // Capture Mode Commands
            commands::set_capture_microphone,
            commands::set_capture_system_audio,
            commands::set_capture_screen,
            commands::set_always_on_capture,
            commands::set_queue_frames_for_vlm,
            commands::set_frame_capture_interval,
            commands::configure_knowledge_base,
            commands::get_capture_settings,
            // VLM Processing Commands (Phase 4)
            commands::analyze_pending_frames,
            commands::get_pending_frame_count,
            commands::get_activity_stats,
            commands::get_unsynced_activities,
            commands::sync_to_cloud,
            // Search Commands (Phase 6)
            commands::search_knowledge_base,
            commands::quick_semantic_search,
            commands::get_local_activities,
            // Data Management Commands
            commands::clear_cache,
            commands::export_data,
            // Prompt Management Commands
            commands::list_prompts,
            commands::get_prompt,
            commands::create_prompt,
            commands::update_prompt,
            commands::delete_prompt,
            commands::duplicate_prompt,
            commands::export_prompts,
            commands::import_prompts,
            // Model Configuration Commands
            commands::list_model_configs,
            commands::get_model_config,
            commands::create_model_config,
            commands::refresh_model_availability,
            commands::list_ollama_models,
            // Use Case Commands
            commands::list_use_cases,
            commands::get_resolved_use_case,
            commands::update_use_case_mapping,
            commands::test_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
