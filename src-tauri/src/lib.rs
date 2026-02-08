// noFriction Meetings - Main Library
// Professional macOS meeting transcription app
#![allow(unexpected_cfgs)]

pub mod ai_client;
pub mod capture_engine;
pub mod catch_up_agent;
pub mod chunk_manager;
pub mod clustering;
pub mod commands;
pub mod database;
pub mod dork_mode;
pub mod meeting_notes;

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

// Environment configuration
pub mod env_config;

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
pub mod accessibility_capture;
pub mod accessibility_extractor;
pub mod calendar_client;
pub mod semantic_classifier;
pub mod vision_ocr;

// v2.1.0: Management Suite (Admin Console)
pub mod admin_commands;
pub mod audit_log;
pub mod data_editor;
pub mod storage_manager;

// v2.5.0: Always-On Recording
pub mod ambient_capture;
pub mod continue_prompt;
pub mod interaction_loop;
pub mod meeting_trigger;
pub mod power_manager;
pub mod privacy_filter;
pub mod tray_builder;

// v3.0.0: Obsidian Vault Integration
pub mod obsidian_vault;

use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use capture_engine::CaptureEngine;
use database::DatabaseManager;
// use deepgram_client::DeepgramClient; // Deprecated
use ambient_capture::AmbientCaptureService;
use interaction_loop::InteractionLoop;
use meeting_trigger::MeetingTriggerEngine;
use pinecone_client::PineconeClient;
use power_manager::PowerManager;
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
    pub timeline_builder: Arc<timeline_builder::TimelineBuilder>,
    // v2.1.0: Apple Calendar Integration
    pub calendar_client: Arc<RwLock<calendar_client::CalendarClient>>,
    // v2.5.0: Always-On Recording
    pub power_manager: Arc<PowerManager>,
    pub ambient_capture: Arc<AmbientCaptureService>,
    pub meeting_trigger: Arc<MeetingTriggerEngine>,
    pub interaction_loop: Arc<InteractionLoop>,
    // v2.7.0: Continuous Accessibility Capture
    pub accessibility_capture: Arc<accessibility_capture::AccessibilityCaptureService>,
    // v2.8.0: Dork Mode (Study Mode)
    pub dork_mode_session: Arc<RwLock<Option<dork_mode::DorkModeSession>>>,
    pub ai_client: Arc<RwLock<ai_client::AIClient>>,
    // v3.0.0: Obsidian Vault Integration
    pub vault_manager: Arc<obsidian_vault::VaultManager>,
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

        // Load environment configuration
        log::info!("Loading environment configuration from .env...");
        let _ = emitter.emit("init-step", "Loading Environment Configuration...");
        let env_config = env_config::EnvConfig::load();
        log::info!("Environment configuration loaded.");

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

        // Auto-populate settings from .env if not already set or empty
        if saved_settings
            .supabase_connection_string
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            if let Some(ref conn_str) = env_config.supabase_connection_string {
                log::info!("✓ Auto-populating Supabase connection string from .env");
                let _ = settings.set_supabase_connection(conn_str).await;
            }
        }
        if saved_settings
            .pinecone_api_key
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            if let Some(ref api_key) = env_config.pinecone_api_key {
                log::info!("✓ Auto-populating Pinecone API key from .env");
                let _ = settings.set_pinecone_api_key(api_key).await;
            }
        }
        if saved_settings
            .pinecone_index_host
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            if let Some(ref host) = env_config.pinecone_index_host {
                log::info!("✓ Auto-populating Pinecone index host from .env");
                let _ = settings.set_pinecone_index_host(host).await;
            }
        }
        if saved_settings
            .pinecone_namespace
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            if let Some(ref ns) = env_config.pinecone_namespace {
                log::info!("✓ Auto-populating Pinecone namespace from .env");
                let _ = settings.set_pinecone_namespace(ns).await;
            }
        }
        if saved_settings
            .vlm_base_url
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            if let Some(ref url) = env_config.vlm_base_url {
                log::info!("✓ Auto-populating VLM base URL from .env");
                let _ = settings.set_vlm_base_url(url).await;
            }
        }

        let supabase = SupabaseClient::new();

        let pinecone = Arc::new(RwLock::new(PineconeClient::new()));

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

        // Initialize v2.5.0: Power Manager for Always-On Recording
        log::info!("Initializing Power Manager...");
        let power_manager = PowerManager::new();

        // Initialize v2.5.0: Ambient Capture Service
        log::info!("Initializing Ambient Capture Service...");
        let ambient_capture = AmbientCaptureService::new();

        // Initialize v2.5.0: Meeting Trigger Engine
        log::info!("Initializing Meeting Trigger Engine...");
        let meeting_trigger = MeetingTriggerEngine::new();

        // Initialize v2.5.0: Interaction Loop for human check-ins
        log::info!("Initializing Interaction Loop...");
        let interaction_loop = InteractionLoop::new();

        // Initialize v2.7.0: Accessibility Capture Service
        log::info!("Initializing Accessibility Capture Service...");
        let accessibility_capture =
            Arc::new(accessibility_capture::AccessibilityCaptureService::new());

        // Auto-start accessibility capture if enabled
        if saved_settings.accessibility_capture_enabled {
            log::info!("Starting Accessibility Capture (enabled in settings)...");
            let acc_cap = accessibility_capture.clone();
            let db_clone = database.clone();
            let settings_clone = settings.clone();
            let pinecone_clone = pinecone.clone();
            tokio::spawn(async move {
                if let Err(e) = acc_cap.start(db_clone, settings_clone, pinecone_clone) {
                    log::warn!("Failed to start accessibility capture: {}", e);
                }
            });
        }

        Ok(Self {
            capture_engine: Arc::new(RwLock::new(capture)),
            // deepgram_client: Arc::new(RwLock::new(deepgram)),
            transcription_manager,
            database,
            settings: settings.clone(),
            vlm_client: Arc::new(RwLock::new(vlm)),
            vlm_scheduler: Arc::new(vlm_scheduler),
            supabase_client: Arc::new(RwLock::new(supabase)),
            pinecone_client: pinecone,
            prompt_manager,
            ingest_client,
            ingest_queue: Arc::new(parking_lot::Mutex::new(ingest_queue)),
            state_builder: Arc::new(RwLock::new(state_builder)),
            metrics_collector: Arc::new(metrics_collector),
            episode_builder: Arc::new(RwLock::new(episode_builder)),
            timeline_builder: Arc::new(timeline_builder),
            calendar_client: Arc::new(RwLock::new(calendar_client::CalendarClient::new())),
            power_manager: Arc::new(power_manager),
            ambient_capture: Arc::new(ambient_capture),
            meeting_trigger: Arc::new(meeting_trigger),
            interaction_loop: Arc::new(interaction_loop),
            accessibility_capture,
            // v2.8.0: Dork Mode (Study Mode)
            dork_mode_session: Arc::new(RwLock::new(None)),
            ai_client: Arc::new(RwLock::new(ai_client::AIClient::new())),
            // v3.0.0: Obsidian Vault Integration
            vault_manager: {
                let vm = Arc::new(obsidian_vault::VaultManager::new());
                // Load vault path from settings if configured
                let settings_clone = settings.clone();
                let vm_clone = vm.clone();
                tokio::spawn(async move {
                    if let Ok(saved_settings) = settings_clone.get_all().await {
                        if let Some(vault_path) = saved_settings.obsidian_vault_path {
                            vm_clone.set_vault_path(vault_path);
                        }
                    }
                });
                vm
            },
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

            // Create system tray with context menu
            if let Err(e) = tray_builder::create_tray(app.handle()) {
                log::error!("Failed to create system tray: {}", e);
            } else {
                log::info!("System tray with context menu created");
            }

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
            // TheBrain Cloud API Commands
            commands::thebrain_authenticate,
            commands::check_thebrain,
            commands::set_vlm_api_url,
            commands::get_thebrain_models,
            commands::capture_accessibility_snapshot,
            commands::thebrain_chat,
            commands::thebrain_rag_chat,
            commands::store_conversation,
            commands::get_conversation_history,
            commands::thebrain_rag_chat_with_memory,
            commands::configure_supabase,
            commands::check_supabase,
            commands::sync_activity_to_supabase,
            commands::query_activities,
            commands::configure_pinecone,
            commands::check_pinecone,
            commands::upsert_to_pinecone,
            commands::semantic_search,
            commands::get_pinecone_stats,
            commands::index_meeting_transcripts,
            commands::index_all_transcripts_to_pinecone,
            commands::get_accessibility_snapshots,
            commands::get_meeting_timeline,
            // Capture Mode Commands
            commands::set_capture_microphone,
            commands::set_capture_system_audio,
            commands::set_capture_screen,
            commands::set_always_on_capture,
            commands::set_queue_frames_for_vlm,
            commands::set_frame_capture_interval,
            commands::configure_knowledge_base,
            commands::configure_knowledge_base,
            commands::get_capture_settings,
            // AI Provider Settings
            commands::set_ai_provider_settings,
            commands::get_ai_provider_settings,
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
            // Accessibility Capture Commands
            commands::get_accessibility_capture_status,
            commands::start_accessibility_capture,
            commands::stop_accessibility_capture,
            commands::set_accessibility_meeting_id,
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
            // v2.5.0: Always-On Recording Commands
            commands::get_capture_mode,
            commands::start_ambient_capture,
            commands::start_meeting_capture,
            commands::pause_capture,
            commands::get_always_on_settings,
            commands::set_always_on_enabled,
            commands::get_running_meeting_apps,
            commands::check_audio_usage,
            commands::dismiss_meeting_detection,
            commands::set_genie_mode,
            // v2.8.0: Dork Mode (Study Mode) Commands
            commands::set_session_mode,
            commands::get_session_mode,
            commands::start_dork_session,
            commands::add_dork_content,
            commands::end_dork_session,
            commands::get_study_materials,
            // v3.0.0: Obsidian Vault Commands
            commands::get_vault_status,
            commands::list_vault_topics,
            commands::get_vault_topic,
            commands::create_vault_topic,
            commands::export_meeting_to_vault,
            commands::read_vault_file,
            commands::write_vault_note,
            commands::upload_to_vault,
            commands::list_vault_files,
            commands::search_vault,
            commands::get_vault_tree,
            commands::delete_vault_item,
            commands::set_vault_path,
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    #[cfg(target_os = "macos")]
                    {
                        // Hide the window instead of closing
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        }
                        api.prevent_close();
                    }
                }
                tauri::WindowEvent::Resized(_) => {
                    #[cfg(target_os = "macos")]
                    {
                        if window.is_minimized().unwrap_or(false) {
                            // Instead of standard minimizing, enter Genie mode
                            let window_clone = window.clone();
                            tauri::async_runtime::spawn(async move {
                                // Emit event to frontend to update state
                                let _ = window_clone.emit("enter-genie-mode", ());
                            });
                        }
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            tauri::RunEvent::Reopen { .. } => {
                #[cfg(target_os = "macos")]
                {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
            _ => {}
        });
}
