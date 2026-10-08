// noFriction Meetings - Main Library
// Professional macOS meeting transcription app
#![allow(unexpected_cfgs)]

pub mod ai;
pub mod ai_client;
pub mod build_info;
pub mod bookmarks;
pub mod core_audio;
pub mod entitlement;
pub mod paths;
pub mod secrets;
pub mod store;
pub mod audio_mixer;
pub mod attendee_intel;
pub mod capture_engine;
pub mod catch_up_agent;
// m1: ffmpeg-based screen video + frame extraction can't run in the App
// Sandbox (no Homebrew binaries). Screenshots (capture_engine/xcap) still
// feed frames and the timeline in the `mas` build.
#[cfg(not(feature = "mas"))]
pub mod chunk_manager;
pub mod clustering;
pub mod commands;
pub mod database;
pub mod dork_mode;
pub mod meeting_notes;

#[cfg(not(feature = "mas"))]
pub mod frame_extractor;
pub mod live_intel_agent;
pub mod meeting_intel;
pub mod menu_builder;
pub mod prompt_manager;
pub mod settings;
pub mod transcription; // New module
#[cfg(not(feature = "mas"))]
pub mod video_recorder;
pub mod vlm_client;
pub mod vlm_scheduler;

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
pub mod meeting_end;
pub mod notifications;
// Record sheet: "What is it?" (recording type), "How long?" (timed
// recording) and the optional notebook
pub mod notebooks;
pub mod recording_kind;
pub mod timed_recording;
pub mod power_manager;
pub mod privacy_filter;
pub mod tray_builder;

// v3.0.0: Obsidian Vault Integration
pub mod obsidian_vault;
pub mod people;
// Transcript/screen editing + "Strike from the record" (docs/REDACTION.md)
pub mod redaction;
// Links & References on every meeting (docs/LINKS.md)
pub mod meeting_links;
// Students: moment markers and study guides (docs/STUDY_TOOLS.md)
pub mod markers;
pub mod study;
// Topics and chat with your recordings (docs/TOPICS_AND_CHAT.md)
pub mod chat;
pub mod topics;

use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use capture_engine::CaptureEngine;
use database::DatabaseManager;

use ambient_capture::AmbientCaptureService;
use interaction_loop::InteractionLoop;
use live_intel_agent::LiveIntelAgent;
use power_manager::PowerManager;
use prompt_manager::PromptManager;
use settings::SettingsManager;
use transcription::TranscriptionManager;

/// Application state shared across commands
pub struct AppState {
    pub capture_engine: Arc<RwLock<CaptureEngine>>,

    pub transcription_manager: Arc<TranscriptionManager>, // New
    pub database: Arc<DatabaseManager>,
    pub settings: Arc<SettingsManager>,
    pub vlm_scheduler: Arc<vlm_scheduler::VLMScheduler>,
    pub prompt_manager: Arc<PromptManager>,
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
    pub interaction_loop: Arc<InteractionLoop>,
    // v2.7.0: Continuous Accessibility Capture
    pub accessibility_capture: Arc<accessibility_capture::AccessibilityCaptureService>,
    // v2.8.0: Dork Mode (Study Mode)
    pub dork_mode_session: Arc<RwLock<Option<dork_mode::DorkModeSession>>>,
    pub ai_client: Arc<RwLock<ai_client::AIClient>>,
    // v3.0.0: Obsidian Vault Integration
    pub vault_manager: Arc<obsidian_vault::VaultManager>,
    pub live_intel_agent: Arc<RwLock<LiveIntelAgent>>,
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

        // A Delete still inside its undo window when the app quit or crashed
        // is committed now: it is never silently dropped (docs/REDACTION.md).
        // Database work only (fast): any screen video blanking it queues runs
        // in the background worker once the app is up.
        for e in redaction::commit_all_pending(database.pool(), &redaction::RedactionEnv::for_app()).await {
            log::error!("Pending delete could not be applied: {}", e);
        }

        // Meetings left open by a previous run (quit/crash mid-recording, or
        // recorded before stop marked meetings ended) get ended_at = their
        // last captured moment. Nothing can be recording yet. Never deletes.
        match database.close_stale_meetings().await {
            Ok(0) => {}
            Ok(n) => log::info!("Closed {} meeting(s) left open by a previous run", n),
            Err(e) => log::warn!("Could not close stale meetings: {}", e),
        }

        // Initialize settings manager (uses same pool)
        log::info!("Initializing Settings Manager...");
        let _ = emitter.emit("init-step", "Loading User Settings...");
        let settings = SettingsManager::new(database.get_pool());
        settings.init().await?;
        log::info!("Settings Manager initialized.");

        // One-time: move any plaintext secrets (SQLite rows, legacy
        // ~/.nofriction-meetings/.env) into the Keychain and delete the
        // plaintext copies. Idempotent; a no-op once done.
        let _ = emitter.emit("init-step", "Securing API keys...");
        secrets::migrate_settings(&settings).await;
        #[cfg(not(feature = "mas"))]
        secrets::migrate_home_env();
        // One-time: the app is client-only; drop Supabase/Pinecone/ingest
        // credentials and settings left by older versions. Idempotent.
        secrets::purge_removed_services(&settings).await;

        // Load saved settings
        let saved_settings = settings.get_all().await.unwrap_or_default();
        log::info!("Settings loaded.");

        // Initialize Transcription Manager (Replaces DeepgramClient)
        log::info!("Initializing Transcription Manager...");
        let _ = emitter.emit("init-step", "Initializing Transcription Service...");
        let transcription_manager = Arc::new(TranscriptionManager::new());

        // Configure local (offline) transcription: models live in app data
        let whisper_models_dir = app_data_dir.join("models");
        let _ = std::fs::create_dir_all(&whisper_models_dir);
        transcription::local_whisper::configure(
            whisper_models_dir,
            saved_settings.local_whisper_model.clone(),
        );

        // Cloud transcription presets are retired. Saved settings/keys are
        // preserved, but the manager can instantiate only local Whisper.
        log::info!("Transcription: local Whisper; legacy service configuration is inactive");

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
        let saved_targets = commands::capture_sources::load_persisted_targets(&settings).await;
        if !saved_targets.is_empty() {
            capture.set_capture_targets(saved_targets);
        }

        // AI provider layer (bring-your-own-key; see docs/AI_PROVIDERS.md).
        // Non-secret config from settings; keys from the Keychain.
        ai::config::init(Arc::new(SettingsManager::new(database.get_pool()))).await;
        let ai_client = ai_client::AIClient::new();

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

        // Initialize v2.5.0: Interaction Loop for human check-ins
        log::info!("Initializing Interaction Loop...");
        let interaction_loop = InteractionLoop::new();

        // Initialize v2.7.0: Accessibility Capture Service
        log::info!("Initializing Accessibility Capture Service...");
        let accessibility_capture =
            Arc::new(accessibility_capture::AccessibilityCaptureService::new());

        // Auto-start accessibility capture if enabled (m2: never in the
        // sandboxed build, where the AX API is compiled out)
        if saved_settings.accessibility_capture_enabled && build_info::ACCESSIBILITY_CAPTURE {
            log::info!("Starting Accessibility Capture (enabled in settings)...");
            let acc_cap = accessibility_capture.clone();
            let db_clone = database.clone();
            let settings_clone = settings.clone();
            tokio::spawn(async move {
                if let Err(e) = acc_cap.start(db_clone, settings_clone) {
                    log::warn!("Failed to start accessibility capture: {}", e);
                }
            });
        }

        // Initialize Live Intelligence Agent
        log::info!("Initializing Live Intelligence Agent...");
        let live_intel_agent = Arc::new(RwLock::new(LiveIntelAgent::new()));

        // Wire up audio callback to TranscriptionManager
        let tm_clone = transcription_manager.clone();
        let mixer = Arc::new(crate::audio_mixer::AudioMixer::new());
        capture.set_audio_callback(Arc::new(move |buffer| {
            // Mic + system audio are mixed into one 16kHz stream; the provider
            // handles buffering/dropping based on its own connection state.
        // Forwarded under the mixer lock so chunks stay in order (non-blocking)
        mixer.push_with(buffer.source, &buffer.samples, buffer.sample_rate, buffer.channels, |mixed| {
            tm_clone.process_audio(mixed, crate::audio_mixer::MIX_SAMPLE_RATE, 1);
        });
        }));

        Ok(Self {
            capture_engine: Arc::new(RwLock::new(capture)),

            transcription_manager,
            database,
            settings: settings.clone(),
            vlm_scheduler: Arc::new(vlm_scheduler),
            prompt_manager,
            state_builder: Arc::new(RwLock::new(state_builder)),
            metrics_collector: Arc::new(metrics_collector),
            episode_builder: Arc::new(RwLock::new(episode_builder)),
            timeline_builder: Arc::new(timeline_builder),
            calendar_client: Arc::new(RwLock::new(calendar_client::CalendarClient::new())),
            power_manager: Arc::new(power_manager),
            ambient_capture: Arc::new(ambient_capture),
            interaction_loop: Arc::new(interaction_loop),
            accessibility_capture,
            // v2.8.0: Dork Mode (Study Mode)
            dork_mode_session: Arc::new(RwLock::new(None)),
            ai_client: Arc::new(RwLock::new(ai_client)),
            // v3.0.0: Obsidian Vault Integration
            vault_manager: {
                let vm = Arc::new(obsidian_vault::VaultManager::new());
                // Load vault path from settings if configured
                let settings_clone = settings.clone();
                let vm_clone = vm.clone();
                tokio::spawn(async move {
                    // m6: prefer the security-scoped bookmark (required in the
                    // sandbox); fall back to the plain path (DMG build).
                    if let Some(vault_path) = commands::vault::restore_vault_access(&settings_clone).await {
                        vm_clone.set_vault_path(vault_path);
                    }
                });
                vm
            },
            live_intel_agent,
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

/// Writes log output to both stderr and a rotating file. When launched as a
/// .app bundle, stderr goes nowhere — the log file is the only way to see
/// what the app did (<app data dir>/logs/app.log; in the sandbox that is
/// ~/Library/Containers/com.nofriction.meetings/Data/Library/Application Support/com.nofriction.meetings/logs).
struct TeeWriter {
    file: Option<std::fs::File>,
}

impl std::io::Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(f) = self.file.as_mut() {
            let _ = f.write_all(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(f) = self.file.as_mut() {
            let _ = f.flush();
        }
        Ok(())
    }
}

fn open_log_file() -> Option<std::fs::File> {
    let dir = paths::logs_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("app.log");
    // Simple size cap: start fresh past 5MB
    if path.metadata().map(|m| m.len() > 5_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("app.log.1"));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before anything opens files under the data folder (the log file
    // included): move the pre-3.6 `ai.nofriction.meetings` folder to the new
    // identifier's folder. DMG build only; never deletes data.
    let data_migration = paths::migrate_legacy_data_dir();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(TeeWriter {
            file: open_log_file(),
        })))
        .init();
    log::info!(
        "──── noFriction starting (v{}) ────",
        env!("CARGO_PKG_VERSION")
    );
    log::info!("Build flavor: {}", build_info::FLAVOR);
    paths::log_migration(&data_migration);
    // The removed ingest feature left ingest_queue.db behind: archive it
    // into <app data>/backups (never deleted).
    paths::archive_removed_ingest_queue_at_startup();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Shortcuts are registered at runtime (markers::register_hotkey), so a
        // key another app owns can't fail the app's setup
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            ai::set_app_handle(handle.clone());
            // ⌃⌥⌘M: mark the moment while another app is in front
            markers::register_hotkey(&handle);
            // Notification clicks (app activation) bring the window back
            notifications::init(&handle);
            // StoreKit: start the Transaction.updates listener and load the
            // current entitlement (no-op outside the `mas` build).
            store::start(handle.clone());
            let caps = build_info::capabilities();
            log::info!(
                "Apple on-device model: {}",
                if caps.apple_intelligence { "available".to_string() } else { format!("unavailable ({})", caps.apple_intelligence_reason) }
            );

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
                                    // Screen video blanking left over from a
                                    // previous run (resumed in the background)
                                    redaction::commands::start_video_worker(&handle_clone);
                                    // "Live insights during meetings" switch
                                    {
                                        let h = handle_clone.clone();
                                        tauri::async_runtime::spawn(async move {
                                            if let Some(st) = h.try_state::<AppState>() {
                                                commands::intel::load_ai_automation(&st).await;
                                            }
                                        });
                                    }

                                    // Update status to Ready
                                    *init_state_clone.write() = InitStatus::Ready;

                                    log::info!("State managed, emitting app-ready...");
                                    let _ = handle_clone.emit("app-ready", ());
                                    // Tray checkbox mirrors the auto-stop setting
                                    {
                                        let h = handle_clone.clone();
                                        tauri::async_runtime::spawn(async move {
                                            if let Ok(s) = meeting_end::get_auto_stop_settings(h.clone()).await {
                                                tray_builder::set_auto_stop_checked(&h, s.enabled);
                                            }
                                        });
                                    }
                                    commands::people::spawn_startup_sync(&handle_clone);
                                    // Tray "Start Recording (60 min)" shows the remembered length
                                    {
                                        let h = handle_clone.clone();
                                        tauri::async_runtime::spawn(async move {
                                            timed_recording::refresh_tray(&h).await;
                                        });
                                    }
                                    log::info!(
                                        "noFriction v{} initialized successfully",
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
            commands::get_microphone_auth_status,
            commands::open_system_settings,
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
            commands::set_active_provider,
            commands::debug_log,
            commands::get_meetings,
            commands::get_meeting,
            commands::delete_meeting,
            commands::get_settings,
            commands::get_setting,
            commands::set_setting,
            // AI Commands
            commands::get_ai_presets,
            commands::ai_chat,
            commands::summarize_meeting,
            commands::extract_action_items,
            // Knowledge Base Commands
            commands::check_vlm,
            commands::check_vlm_vision,
            commands::analyze_frame,
            commands::analyze_frames_batch,
            // Assistant chat (active AI provider)
            commands::capture_accessibility_snapshot,
            commands::assistant_chat,
            commands::assistant_rag_chat,
            commands::store_conversation,
            commands::get_conversation_history,
            commands::assistant_rag_chat_with_memory,
            commands::get_accessibility_snapshots,
            commands::get_meeting_timeline,
            // Capture Mode Commands
            commands::set_capture_microphone,
            commands::set_capture_system_audio,
            commands::set_capture_screen,
            commands::set_always_on_capture,
            commands::set_queue_frames_for_vlm,
            commands::set_frame_capture_interval,
            // Capture sources (displays / windows) + on-demand snapshots
            commands::list_capture_sources,
            commands::get_capture_targets,
            commands::set_capture_targets,
            commands::snap_capture_target,
            commands::pause_recording,
            // Calendar-linked people + LinkedIn
            commands::sync_calendar,
            commands::get_calendar_access_status,
            commands::get_meeting_people,
            commands::list_people,
            commands::set_person_linkedin,
            commands::resume_recording,
            // Local (offline) speech-to-text
            commands::get_local_stt_status,
            commands::set_local_whisper_model,
            commands::download_whisper_model,
            commands::delete_whisper_model,
            commands::get_capture_settings,
            // AI Providers (bring your own key; Keychain-backed)
            ai::commands::ai_list_providers,
            ai::commands::ai_detect_provider,
            ai::commands::ai_save_key,
            ai::commands::ai_delete_key,
            ai::commands::ai_set_active,
            ai::commands::ai_clear_vision,
            ai::commands::ai_set_custom_endpoint,
            ai::commands::ai_list_models,
            ai::commands::ai_test,
            ai::commands::ai_grant_consent,
            ai::commands::ai_revoke_consent,
            ai::commands::ai_status,
            // VLM Processing Commands (Phase 4)
            commands::analyze_pending_frames,
            commands::get_pending_frame_count,
            commands::get_activity_stats,
            // Search Commands (Phase 6)
            commands::search_knowledge_base,
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
            // Realtime Transcription (Deepgram)
            commands::start_realtime_transcription,
            // Video Recording Commands
            #[cfg(not(feature = "mas"))]
            commands::start_video_recording,
            #[cfg(not(feature = "mas"))]
            commands::stop_video_recording,
            #[cfg(not(feature = "mas"))]
            commands::get_video_recording_status,
            #[cfg(not(feature = "mas"))]
            commands::video_pin_moment,
            #[cfg(not(feature = "mas"))]
            commands::extract_frame_at,
            #[cfg(not(feature = "mas"))]
            commands::extract_thumbnail,
            #[cfg(not(feature = "mas"))]
            commands::get_storage_stats,
            #[cfg(not(feature = "mas"))]
            commands::apply_retention,
            #[cfg(not(feature = "mas"))]
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
            // Obsidian Knowledge Management
            commands::get_vault_backlinks,
            commands::list_vault_tags,
            commands::get_files_by_tag,
            commands::get_vault_graph,
            // v3.1.0: Calendar Intelligence Commands
            commands::generate_meeting_intel,
            commands::get_enriched_calendar_events,
            // v3.0.0: Calendar Integration — Attendees
            commands::get_meeting_attendees,
            // v3.2.0: Calendar Access, People Lookup, Recording Overlap
            commands::update_meeting_title,
            commands::lookup_attendees,
            commands::match_recording_to_calendar,
            // v3.4.0: Meeting Report Prompt Management
            commands::get_meeting_report_prompt,
            commands::set_meeting_report_prompt,
            commands::generate_meeting_report,
            commands::get_ai_automation,
            commands::draft_followup_email,
            commands::set_ai_automation,
            // Meeting notes (Recordings → Notes)
            commands::generate_meeting_notes,
            commands::get_meeting_notes,
            // Editing + "Strike from the record"
            redaction::commands::delete_transcript_words,
            redaction::commands::strike_transcript_words,
            redaction::commands::delete_transcript_line,
            redaction::commands::strike_transcript_line,
            redaction::commands::delete_screens,
            redaction::commands::strike_screens,
            redaction::commands::undo_redaction,
            redaction::commands::commit_redaction,
            redaction::commands::list_failed_redactions,
            redaction::commands::retry_failed_redactions,
            redaction::commands::list_redactions,
            redaction::commands::preview_strike_words,
            redaction::commands::preview_strike_screens,
            redaction::commands::preview_time_range,
            redaction::commands::delete_time_range,
            redaction::commands::strike_time_range,
            redaction::commands::preview_time_ranges,
            redaction::commands::delete_time_ranges,
            redaction::commands::strike_time_ranges,
            redaction::commands::list_video_blank_jobs,
            redaction::commands::retry_video_blank_jobs,
            redaction::commands::get_meeting_ai_status,
            // Links & References (docs/LINKS.md)
            meeting_links::commands::list_meeting_links,
            meeting_links::commands::add_meeting_reference,
            meeting_links::commands::update_meeting_reference,
            meeting_links::commands::delete_meeting_reference,
            meeting_links::commands::hide_meeting_link,
            meeting_links::commands::open_meeting_link,
            meeting_links::commands::get_browser_url_capture,
            meeting_links::commands::set_browser_url_capture,
            // Moment markers + study guides (docs/STUDY_TOOLS.md)
            markers::commands::mark_moment,
            markers::commands::add_marker,
            markers::commands::update_marker,
            markers::commands::delete_marker,
            markers::commands::list_markers,
            study::commands::get_study_guide,
            study::commands::generate_study_guide,
            study::commands::export_study_flashcards,
            study::commands::export_study_guide,
            // Topics and chat with your recordings (docs/TOPICS_AND_CHAT.md)
            topics::commands::list_topics,
            topics::commands::get_meeting_topics,
            topics::commands::set_meeting_topics,
            topics::commands::find_topics,
            chat::commands::chat_ask,
            chat::commands::list_chat_threads,
            chat::commands::get_chat_thread,
            chat::commands::delete_chat_thread,
            chat::commands::chat_scope_summary,
            // Build flavor / capabilities (UI hides features the build lacks)
            build_info::get_build_capabilities,
            // StoreKit (Mac App Store build; DMG returns "not available")
            store::store_products,
            store::store_purchase,
            store::store_entitlement,
            store::store_restore,
            store::store_manage_subscriptions,
            // Meeting-end detection (auto-stop)
            meeting_end::meeting_end_keep_recording,
            meeting_end::get_meeting_end_status,
            meeting_end::get_auto_stop_settings,
            meeting_end::set_auto_stop_settings,
            // Record sheet: type, timed recording, notebook
            timed_recording::get_record_prefs,
            timed_recording::get_timed_recording_status,
            timed_recording::extend_timed_recording,
            timed_recording::remove_timed_recording_limit,
            notebooks::set_meeting_notebook,
            notebooks::list_recent_notebooks,
            recording_kind::set_meeting_recording_kind,
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
            tauri::RunEvent::Exit => {
                // A running screen video job stops here and resumes at launch
                redaction::video_jobs::shutdown();
                // Deletes still in their 5s undo window are committed on quit
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let db = state.database.clone();
                    // Quitting mid-recording: close the meeting row now
                    let open_meeting = state.state_builder.read().current_meeting_id();
                    if let Some(mid) = open_meeting {
                        if let Err(e) = tauri::async_runtime::block_on(db.end_meeting(&mid)) {
                            log::warn!("Could not close meeting {} on quit: {}", mid, e);
                        } else {
                            log::info!("Closed in-progress meeting {} on quit", mid);
                        }
                    }
                    let errors = tauri::async_runtime::block_on(redaction::commit_all_pending(
                        db.pool(),
                        &redaction::RedactionEnv::for_app(),
                    ));
                    for e in errors {
                        log::error!("Pending delete could not be applied on quit: {}", e);
                    }
                }
            }
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
