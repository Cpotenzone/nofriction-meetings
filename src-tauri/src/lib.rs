// noFriction Meetings - Main Library
// Professional macOS meeting transcription app

pub mod ai_client;
pub mod capture_engine;
pub mod catch_up_agent;
pub mod chunk_manager;
pub mod commands;
pub mod database;

// pub mod deepgram_client; // Deprecated
pub mod frame_extractor;
pub mod live_intel_agent;
pub mod meeting_intel;
pub mod menu_builder;
pub mod pinecone_client;
pub mod prompt_manager;
pub mod settings;
pub mod supabase_client;
pub mod transcription; // New module
pub mod video_recorder;
pub mod vlm_client;
pub mod vlm_scheduler;

// Intelligence Pipeline integration
pub mod ingest_client;
pub mod ingest_queue;

// Phase 1: Stateful Screen Ingest
pub mod capture_metrics;
pub mod dedupe_gate;
pub mod state_builder;

// Phase 2: Episodes & Text Snapshots
pub mod diff_builder;
pub mod episode_builder;
pub mod snapshot_extractor;

// Phase 3: Timeline & Accessibility
pub mod timeline_builder;

// v2.1.0: Native Text Extraction & Classification
pub mod accessibility_extractor;
pub mod calendar_client;
pub mod semantic_classifier;
pub mod vision_ocr;

// v2.1.0: Management Suite (Admin Console)
pub mod admin_commands;
pub mod audit_log;
pub mod data_editor;
pub mod storage_manager;

use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use capture_engine::CaptureEngine;
use database::DatabaseManager;
// use deepgram_client::DeepgramClient; // Deprecated
use pinecone_client::PineconeClient;
use prompt_manager::PromptManager;
use settings::SettingsManager;
use supabase_client::SupabaseClient;
use transcription::TranscriptionManager;
use vlm_client::VLMClient;

/// Application state shared across commands
pub struct AppState {
    pub capture_engine: Arc<RwLock<CaptureEngine>>,
    // pub deepgram_client: Arc<RwLock<DeepgramClient>>, // Deprecated
    pub transcription_manager: Arc<TranscriptionManager>, // New
    pub database: Arc<DatabaseManager>,
    pub settings: Arc<SettingsManager>,
    pub vlm_client: Arc<RwLock<VLMClient>>,
    pub vlm_scheduler: Arc<vlm_scheduler::VLMScheduler>,
    pub supabase_client: Arc<RwLock<SupabaseClient>>,
    pub pinecone_client: Arc<RwLock<PineconeClient>>,
    pub prompt_manager: Arc<PromptManager>,
    // Intelligence Pipeline integration
    pub ingest_client: Option<Arc<ingest_client::IngestClient>>,
    pub ingest_queue: Arc<parking_lot::Mutex<ingest_queue::IngestQueue>>,
    // Phase 1: Stateful Screen Ingest
    pub state_builder: Arc<RwLock<state_builder::StateBuilder>>,
    pub metrics_collector: Arc<capture_metrics::MetricsCollector>,
    // Phase 2: Episode Building
    pub episode_builder: Arc<RwLock<episode_builder::EpisodeBuilder>>,
    // Phase 3: Timeline Generation
    pub timeline_builder: Arc<timeline_builder::TimelineBuilder>,
    // v2.1.0: Apple Calendar Integration
    pub calendar_client: Arc<RwLock<calendar_client::CalendarClient>>,
}

impl AppState {
    pub async fn new(
        app: &AppHandle,
        emitter: &AppHandle,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Get app data directory for SQLite database
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;

        std::fs::create_dir_all(&app_data_dir)?;

        let db_path = app_data_dir.join("nofriction_meetings.db");

        // Initialize database
        log::info!("Initializing Database Manager...");
        let _ = emitter.emit("init-step", "Connecting to SQLite Database...");
        let database = DatabaseManager::new(&db_path).await?;

        log::info!("Running Database Migrations...");
        let _ = emitter.emit("init-step", "Running Database Migrations...");
        database.run_migrations().await?;
        log::info!("Database initialized.");

        // Initialize settings manager (uses same pool)
        log::info!("Initializing Settings Manager...");
        let _ = emitter.emit("init-step", "Loading User Settings...");
        let settings = SettingsManager::new(database.get_pool());
        settings.init().await?;
        log::info!("Settings Manager initialized.");

        // Load saved settings
        let saved_settings = settings.get_all().await.unwrap_or_default();
        log::info!("Settings loaded.");

        // Initialize Transcription Manager (Replaces DeepgramClient)
        log::info!("Initializing Transcription Manager...");
        let _ = emitter.emit("init-step", "Initializing Transcription Service...");
        let transcription_manager = Arc::new(TranscriptionManager::new());

        // Configure transcription provider
        if let Some(ref api_key) = saved_settings.deepgram_api_key {
            transcription_manager.set_api_key(api_key.clone());
        }

        // Load other keys if present (placeholder for now)
        // TODO: In full implementation, load provider choice and other keys

        // Initialize capture engine with saved preferences
        log::info!("Initializing Capture Engine...");
        let _ = emitter.emit("init-step", "Initializing Capture Engine...");
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
        log::info!("Initializing Knowledge Base Clients...");
        let _ = emitter.emit("init-step", "Connecting to Knowledge Base...");
        let vlm = VLMClient::new();

        // Configure VLM client with saved settings
        if let Some(ref base_url) = saved_settings.vlm_base_url {
            vlm.set_base_url(base_url.clone());
            log::info!("VLM configured with base URL: {}", base_url);
        }
        if let Some(ref token) = saved_settings.vlm_bearer_token {
            vlm.set_bearer_token(token.clone());
            log::info!("VLM bearer token configured");
        }
        if let Some(ref model) = saved_settings.vlm_model_primary {
            vlm.set_model(model.clone());
        }
        if let Some(ref model) = saved_settings.vlm_model_fallback {
            vlm.set_fallback_model(model.clone());
        }

        // Also configure the global VLM client for standalone functions
        if let Some(ref base_url) = saved_settings.vlm_base_url {
            crate::vlm_client::vlm_configure(base_url, saved_settings.vlm_bearer_token.as_deref());
        }

        let supabase = SupabaseClient::new();
        let pinecone = PineconeClient::new();

        // Initialize prompt manager with same pool
        log::info!("Initializing Prompt Manager...");
        let _ = emitter.emit("init-step", "Loading Prompt Library...");
        let prompt_manager = Arc::new(PromptManager::new((*database.get_pool()).clone()));
        prompt_manager.run_migrations().await?;
        log::info!("Prompt manager initialized with default presets");

        log::info!("Database initialized at: {:?}", db_path);

        // Wrap in Arc for sharing
        let database = Arc::new(database);
        let settings = Arc::new(settings);

        // Initialize VLM scheduler
        log::info!("Initializing VLM Scheduler...");
        let _ = emitter.emit("init-step", "Starting VLM Scheduler...");
        let vlm_scheduler = vlm_scheduler::VLMScheduler::new(
            database.clone(),
            settings.clone(),
            prompt_manager.clone(),
        );

        // Load VLM scheduler settings and start if enabled
        if saved_settings.vlm_auto_process {
            vlm_scheduler.set_enabled(true);
            vlm_scheduler.set_interval(saved_settings.vlm_process_interval_secs);
            vlm_scheduler.start();
            log::info!(
                "VLM Scheduler started with {}s interval",
                saved_settings.vlm_process_interval_secs
            );
        }

        log::info!("AppState initialization complete.");

        // Initialize Intelligence Pipeline integration
        log::info!("Initializing Intelligence Pipeline integration...");
        let _ = emitter.emit("init-step", "Setting up Intelligence Pipeline...");

        // Initialize ingest queue (always available for local queueing)
        let queue_path = app_data_dir.join("ingest_queue.db");
        let ingest_queue = ingest_queue::IngestQueue::new(&queue_path)
            .map_err(|e| format!("Failed to initialize ingest queue: {}", e))?;

        // Initialize ingest client if enabled
        let ingest_client = if saved_settings.enable_ingest.unwrap_or(false) {
            if let (Some(base_url), Some(bearer_token)) = (
                saved_settings.ingest_base_url.as_ref(),
                saved_settings.ingest_bearer_token.as_ref(),
            ) {
                log::info!("Ingest enabled, creating client for: {}", base_url);
                Some(Arc::new(ingest_client::IngestClient::new(
                    base_url.clone(),
                    bearer_token.clone(),
                )))
            } else {
                log::warn!("Ingest enabled but missing configuration");
                None
            }
        } else {
            log::info!("Ingest disabled");
            None
        };

        // Initialize Phase 1: Stateful Screen Ingest components
        log::info!("Initializing Stateful Screen Ingest (v2.0)...");
        let _ = emitter.emit("init-step", "Setting up Stateful Capture Pipeline...");
        let state_builder = state_builder::StateBuilder::new();
        let metrics_collector = capture_metrics::MetricsCollector::new();

        // Initialize Phase 2: Episode Building
        let episode_builder = episode_builder::EpisodeBuilder::new();

        // Initialize Phase 3: Timeline Building
        let timeline_builder = timeline_builder::TimelineBuilder::new();
        log::info!("Stateful Screen Ingest initialized (Phase 1-3).");

        Ok(Self {
            capture_engine: Arc::new(RwLock::new(capture)),
            // deepgram_client: Arc::new(RwLock::new(deepgram)),
            transcription_manager,
            database,
            settings,
            vlm_client: Arc::new(RwLock::new(vlm)),
            vlm_scheduler: Arc::new(vlm_scheduler),
            supabase_client: Arc::new(RwLock::new(supabase)),
            pinecone_client: Arc::new(RwLock::new(pinecone)),
            prompt_manager,
            ingest_client,
            ingest_queue: Arc::new(parking_lot::Mutex::new(ingest_queue)),
            state_builder: Arc::new(RwLock::new(state_builder)),
            metrics_collector: Arc::new(metrics_collector),
            episode_builder: Arc::new(RwLock::new(episode_builder)),
            timeline_builder: Arc::new(timeline_builder),
            calendar_client: Arc::new(RwLock::new(calendar_client::CalendarClient::new())),
        })
    }
}

#[derive(Clone, serde::Serialize, Debug)]
pub enum InitStatus {
    Initializing,
    Ready,
    Failed(String),
}

pub struct InitializationState(pub Arc<RwLock<InitStatus>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Initialize AppState synchronously to prevent race conditions
            let init_state = Arc::new(RwLock::new(InitStatus::Initializing));
            app.manage(InitializationState(init_state.clone()));

            // Initialize app state synchronously to prevent race conditions
            let handle_clone = handle.clone();
            let init_state_clone = init_state.clone();

            // Initialize app state asynchronously
            tauri::async_runtime::spawn(async move {
                log::info!("Starting AppState initialization...");

                // Add a timeout to the initialization (15 seconds)
                let init_future = async {
                    let _ = handle_clone.emit("init-step", "Initializing Database Manager...");
                    log::info!("Initializing Database...");

                    // Wrap with timeout
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(15),
                        AppState::new(&handle_clone, &handle_clone), // Pass handle for emitting internal steps
                    )
                    .await;

                    match result {
                        Ok(inner_result) => {
                            match inner_result {
                                Ok(state) => {
                                    let _ =
                                        handle_clone.emit("init-step", "Finalizing App State...");
                                    log::info!("AppState created, managing state...");
                                    handle_clone.manage(state);

                                    // Update status to Ready
                                    *init_state_clone.write() = InitStatus::Ready;

                                    log::info!("State managed, emitting app-ready...");
                                    let _ = handle_clone.emit("app-ready", ());
                                    log::info!(
                                        "noFriction Meetings v{} initialized successfully",
                                        env!("CARGO_PKG_VERSION")
                                    );
                                }
                                Err(e) => {
                                    log::error!("Failed to initialize app state: {}", e);
                                    // Update status to Failed
                                    *init_state_clone.write() = InitStatus::Failed(e.to_string());
                                    let _ = handle_clone.emit("init-error", e.to_string());
                                }
                            }
                        }
                        Err(_) => {
                            let msg =
                                "Initialization timed out after 15 seconds. Check database locks.";
                            log::error!("{}", msg);
                            // Update status to Failed
                            *init_state_clone.write() = InitStatus::Failed(msg.to_string());
                            let _ = handle_clone.emit("init-error", msg.to_string());
                        }
                    }
                };

                // temporary simplified timeout check using tokio select if possible,
                // but for now just logging is enough to see progress in stdout if we run it.
                init_future.await;
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
            commands::check_init_status,
            commands::check_permissions,
            commands::test_screen_capture,
            commands::test_microphone,
            commands::test_accessibility,
            commands::request_permission,
            commands::start_recording,
            commands::stop_recording,
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
            commands::set_gemini_api_key,
            commands::set_gladia_api_key,
            commands::set_google_stt_key,
            commands::set_active_provider,
            commands::debug_log,
            commands::get_meetings,
            commands::get_meeting,
            commands::delete_meeting,
            commands::get_settings,
            commands::get_setting,
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
            // Phase 2: Theme-Specific Prompt Management Commands
            commands::list_prompts_by_theme,
            commands::get_latest_prompt,
            commands::get_prompt_versions,
            commands::create_prompt_version,
            commands::create_theme_prompt,
            // Meeting Intelligence Commands
            commands::get_meeting_state,
            commands::generate_catch_up,
            commands::get_live_insights,
            commands::pin_insight,
            commands::mark_decision,
            // Calendar
            commands::get_calendar_events,
            // Realtime Transcription (Deepgram)
            commands::start_realtime_transcription,
            // Video Recording Commands
            commands::start_video_recording,
            commands::stop_video_recording,
            commands::get_video_recording_status,
            commands::video_pin_moment,
            commands::extract_frame_at,
            commands::extract_thumbnail,
            commands::get_storage_stats,
            commands::apply_retention,
            commands::delete_video_storage,
            // VLM Scheduler Commands
            commands::set_vlm_auto_process,
            commands::set_vlm_process_interval,
            commands::get_vlm_scheduler_status,
            // AI Chat Model Commands
            commands::set_ai_chat_model,
            commands::get_ai_chat_model,
            // Activity Theme Commands
            commands::set_active_theme,
            commands::get_active_theme,
            commands::get_theme_settings,
            commands::set_theme_interval,
            commands::get_theme_time_today,
            // Intel Commands
            commands::get_recent_entities,
            // Intelligence Pipeline Commands
            commands::set_enable_ingest,
            commands::set_ingest_config,
            commands::get_ingest_queue_stats,
            commands::test_ingest_connection,
            commands::trigger_meeting_ingest,
            // Phase 3: Timeline Commands
            commands::get_timeline_events,
            commands::get_topic_clusters,
            // v2.1.0: Calendar Integration Commands
            commands::check_calendar_access,
            commands::request_calendar_access,
            commands::get_calendar_events,
            commands::get_current_meeting,
            commands::get_upcoming_meetings,
            // v2.1.0: Capture Metrics Command
            commands::get_capture_metrics,
            // v2.1.0: Management Suite Commands
            admin_commands::list_recordings_with_storage,
            admin_commands::get_admin_storage_stats,
            admin_commands::preview_delete_recordings,
            admin_commands::delete_recordings,
            admin_commands::get_audit_log,
            admin_commands::get_audit_log_count,
            admin_commands::get_system_health,
            admin_commands::get_admin_queue_stats,
            admin_commands::get_feature_flags,
            admin_commands::set_feature_flag,
            // v2.1.0: Learned Data Commands
            admin_commands::list_learned_data,
            admin_commands::count_learned_data,
            admin_commands::edit_learned_data,
            admin_commands::get_data_versions,
            admin_commands::restore_data_version,
            // v2.1.0: Tools Console Commands (M4)
            admin_commands::get_job_history,
            admin_commands::pause_ingest_queue,
            admin_commands::get_database_stats,
            // v2.1.0: Video Diagnostics Commands
            commands::get_capture_diagnostics,
            commands::test_live_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
