// noFriction Meetings - System Tray Builder
// Creates system tray icon with right-click context menu

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};

/// Tray menu item IDs
pub mod tray_ids {
    // Recording Controls
    pub const START_RECORDING: &str = "tray_start_recording";
    pub const STOP_RECORDING: &str = "tray_stop_recording";
    pub const PAUSE_RECORDING: &str = "tray_pause_recording";

    // Capture Modes
    pub const MODE_AMBIENT: &str = "tray_mode_ambient";
    pub const MODE_MEETING: &str = "tray_mode_meeting";
    pub const MODE_PAUSED: &str = "tray_mode_paused";

    // Quick Actions
    pub const SHOW_WINDOW: &str = "tray_show_window";
    pub const OPEN_INSIGHTS: &str = "tray_open_insights";
    pub const OPEN_KB: &str = "tray_open_kb";
    pub const OPEN_SETTINGS: &str = "tray_open_settings";

    // App Controls
    pub const QUIT: &str = "tray_quit";
}

/// Build the system tray with right-click context menu
pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    const TRAY_ID: &str = "nofriction-main-tray";

    // Check if tray already exists (singleton pattern)
    if app.tray_by_id(TRAY_ID).is_some() {
        log::warn!("⚠️ System tray already exists, skipping creation");
        return Ok(());
    }

    // Build the context menu
    let menu = MenuBuilder::new(app)
        // Header
        .text("nofriction_header", "🎯 noFriction Meetings")
        .separator()
        // Recording Controls
        .item(
            &MenuItemBuilder::with_id(tray_ids::START_RECORDING, "⏺  Start Recording")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::STOP_RECORDING, "⏹  Stop Recording").build(app)?)
        .separator()
        // Capture Mode Submenu
        .text("mode_header", "📡 Capture Mode")
        .item(
            &MenuItemBuilder::with_id(tray_ids::MODE_AMBIENT, "  🌙 Ambient (Background)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(tray_ids::MODE_MEETING, "  🎙️ Meeting (Full Capture)")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::MODE_PAUSED, "  ⏸️ Paused").build(app)?)
        .separator()
        // Quick Access
        .item(
            &MenuItemBuilder::with_id(tray_ids::SHOW_WINDOW, "📺 Show Window")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(tray_ids::OPEN_INSIGHTS, "📊 Activity Insights")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::OPEN_KB, "🔍 Knowledge Base").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::OPEN_SETTINGS, "⚙️ Settings").build(app)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("Quit noFriction"))?)
        .build()?;

    // Create the tray icon with unique ID
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_tray_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                // Left click: show/focus main window
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    log::info!("✅ System tray created with context menu (ID: {})", TRAY_ID);
    Ok(())
}

/// Handle tray menu events
fn handle_tray_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    log::info!("Tray menu event: {}", id);

    match id {
        // Recording Controls
        tray_ids::START_RECORDING => {
            emit_to_frontend(app, "tray:start_recording");
        }
        tray_ids::STOP_RECORDING => {
            emit_to_frontend(app, "tray:stop_recording");
        }
        tray_ids::PAUSE_RECORDING => {
            emit_to_frontend(app, "tray:pause_recording");
        }

        // Capture Modes
        tray_ids::MODE_AMBIENT => {
            emit_to_frontend(app, "menu:mode_ambient");
        }
        tray_ids::MODE_MEETING => {
            emit_to_frontend(app, "menu:mode_meeting");
        }
        tray_ids::MODE_PAUSED => {
            emit_to_frontend(app, "menu:mode_pause");
        }

        // Navigation
        tray_ids::SHOW_WINDOW => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        tray_ids::OPEN_INSIGHTS => {
            emit_to_frontend(app, "menu:insights");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        tray_ids::OPEN_KB => {
            emit_to_frontend(app, "menu:search");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        tray_ids::OPEN_SETTINGS => {
            emit_to_frontend(app, "menu:settings");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }

        _ => {
            log::debug!("Unhandled tray event: {}", id);
        }
    }
}

/// Helper to emit events to frontend
fn emit_to_frontend<R: Runtime>(app: &AppHandle<R>, event: &str) {
    if let Err(e) = app.emit(event, ()) {
        log::error!("Failed to emit {}: {}", event, e);
    }
}
