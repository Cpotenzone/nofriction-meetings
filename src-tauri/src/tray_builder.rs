// noFriction Meetings - System Tray Builder
// Creates system tray icon with right-click context menu

use once_cell::sync::OnceCell;
use tauri::{
    menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, Wry,
};

/// Timed recording items (timed_recording.rs): the start label shows the
/// remembered length; status / +15 / No limit follow the running plan.
struct TimedItems {
    start: MenuItem<Wry>,
    status: MenuItem<Wry>,
    extend: MenuItem<Wry>,
    no_limit: MenuItem<Wry>,
}

static TIMED_ITEMS: OnceCell<TimedItems> = OnceCell::new();

/// "Start Recording (Meeting, 60 min)": the type and length a tray /
/// shortcut start uses (`timed_recording::start_label`).
pub fn set_start_recording_label<R: Runtime>(_app: &AppHandle<R>, what: &str) {
    if let Some(items) = TIMED_ITEMS.get() {
        let _ = items.start.set_text(format!("Start Recording ({})", what));
    }
}

/// Time-limit line, and whether +15 / No limit are actionable.
pub fn set_time_limit_status<R: Runtime>(_app: &AppHandle<R>, text: &str, has_limit: bool) {
    if let Some(items) = TIMED_ITEMS.get() {
        let _ = items.status.set_text(text);
        let _ = items.extend.set_enabled(has_limit);
        let _ = items.no_limit.set_enabled(has_limit);
    }
}

/// Tray submenu id → duration sent with `tray:start_recording`.
pub fn start_duration_for(id: &str) -> Option<&'static str> {
    match id {
        tray_ids::START_15 => Some("15"),
        tray_ids::START_30 => Some("30"),
        tray_ids::START_60 => Some("60"),
        tray_ids::START_90 => Some("90"),
        tray_ids::START_NO_LIMIT => Some("none"),
        _ => None,
    }
}

/// Auto-stop (meeting-end detection) items, updated as state changes.
struct AutoStopItems {
    toggle: CheckMenuItem<Wry>,
    status: MenuItem<Wry>,
    keep: MenuItem<Wry>,
}

static AUTO_STOP_ITEMS: OnceCell<AutoStopItems> = OnceCell::new();

/// Show meeting-end detection state in the tray menu (status line, and
/// whether "Keep Recording" is actionable).
pub fn set_auto_stop_status<R: Runtime>(_app: &AppHandle<R>, text: &str, countdown_active: bool) {
    if let Some(items) = AUTO_STOP_ITEMS.get() {
        let _ = items.status.set_text(text);
        let _ = items.keep.set_enabled(countdown_active);
    }
}

/// Reflect the "Stop When Meeting Ends" setting in the tray checkbox.
pub fn set_auto_stop_checked<R: Runtime>(_app: &AppHandle<R>, enabled: bool) {
    if let Some(items) = AUTO_STOP_ITEMS.get() {
        let _ = items.toggle.set_checked(enabled);
        if !enabled {
            let _ = items.status.set_text("Auto-stop: off");
            let _ = items.keep.set_enabled(false);
        } else if items.status.text().map(|t| t == "Auto-stop: off").unwrap_or(false) {
            let _ = items.status.set_text("Auto-stop: on");
        }
    }
}

/// Tray menu item IDs
pub mod tray_ids {
    // Recording Controls
    pub const START_RECORDING: &str = "tray_start_recording";
    pub const STOP_RECORDING: &str = "tray_stop_recording";
    pub const PAUSE_RECORDING: &str = "tray_pause_recording";
    pub const RESUME_RECORDING: &str = "tray_resume_recording";
    pub const AUTO_STOP_TOGGLE: &str = "tray_auto_stop_toggle";
    pub const AUTO_STOP_STATUS: &str = "tray_auto_stop_status";
    pub const AUTO_STOP_KEEP: &str = "tray_auto_stop_keep";
    // Timed recording: "Start Recording For" submenu, and the running plan
    pub const START_15: &str = "tray_start_15";
    pub const START_30: &str = "tray_start_30";
    pub const START_60: &str = "tray_start_60";
    pub const START_90: &str = "tray_start_90";
    pub const START_NO_LIMIT: &str = "tray_start_no_limit";
    pub const TIME_LIMIT_STATUS: &str = "tray_time_limit_status";
    pub const TIME_LIMIT_EXTEND: &str = "tray_time_limit_extend";
    pub const TIME_LIMIT_REMOVE: &str = "tray_time_limit_remove";

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
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    const TRAY_ID: &str = "nofriction-main-tray";

    // Check if tray already exists (singleton pattern)
    if app.tray_by_id(TRAY_ID).is_some() {
        log::warn!("⚠️ System tray already exists, skipping creation");
        return Ok(());
    }

    // Meeting-end detection (state filled in once settings load)
    let auto_toggle = CheckMenuItemBuilder::with_id(tray_ids::AUTO_STOP_TOGGLE, "Stop When Meeting Ends")
        .checked(true)
        .build(app)?;
    let auto_status = MenuItemBuilder::with_id(tray_ids::AUTO_STOP_STATUS, "Auto-stop: on")
        .enabled(false)
        .build(app)?;
    let auto_keep = MenuItemBuilder::with_id(tray_ids::AUTO_STOP_KEEP, "Keep Recording (cancel auto-stop)")
        .enabled(false)
        .build(app)?;

    // Timed recording: the plain item uses the remembered type and length
    // (label set once settings load); the submenu picks a length (and
    // remembers it) and records the remembered type
    let start_item = MenuItemBuilder::with_id(tray_ids::START_RECORDING, "Start Recording").build(app)?;
    let start_for = SubmenuBuilder::new(app, "Start Recording For")
        .item(&MenuItemBuilder::with_id(tray_ids::START_15, "15 Minutes").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::START_30, "30 Minutes").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::START_60, "60 Minutes").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::START_90, "90 Minutes").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::START_NO_LIMIT, "No Limit").build(app)?)
        .build()?;
    let time_status = MenuItemBuilder::with_id(tray_ids::TIME_LIMIT_STATUS, "Time limit: not recording")
        .enabled(false)
        .build(app)?;
    let time_extend = MenuItemBuilder::with_id(tray_ids::TIME_LIMIT_EXTEND, "Add 15 Minutes")
        .enabled(false)
        .build(app)?;
    let time_remove = MenuItemBuilder::with_id(tray_ids::TIME_LIMIT_REMOVE, "Remove Time Limit")
        .enabled(false)
        .build(app)?;

    // Build the context menu
    let menu = MenuBuilder::new(app)
        // Header
        .text("nofriction_header", "noFriction Meetings")
        .separator()
        // Recording Controls
        .item(&start_item)
        .item(&start_for)
        .item(&MenuItemBuilder::with_id(tray_ids::STOP_RECORDING, "Stop Recording").build(app)?)
        .item(
            &MenuItemBuilder::with_id(tray_ids::PAUSE_RECORDING, "Pause Recording")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(tray_ids::RESUME_RECORDING, "Resume Recording")
                .build(app)?,
        )
        .item(&time_status)
        .item(&time_extend)
        .item(&time_remove)
        .item(&auto_toggle)
        .item(&auto_status)
        .item(&auto_keep)
        .separator()
        // Capture Mode Submenu
        .text("mode_header", "Capture Mode")
        .item(
            &MenuItemBuilder::with_id(tray_ids::MODE_AMBIENT, "Ambient (Background)")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(tray_ids::MODE_MEETING, "Meeting (Full Capture)")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::MODE_PAUSED, "Paused").build(app)?)
        .separator()
        // Quick Access
        .item(
            &MenuItemBuilder::with_id(tray_ids::SHOW_WINDOW, "Show Window")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(tray_ids::OPEN_INSIGHTS, "Activity Insights")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::OPEN_KB, "Knowledge Base").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::OPEN_SETTINGS, "Settings…").build(app)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("Quit noFriction"))?)
        .build()?;

    // Create the tray icon with unique ID
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        // Monochrome template glyph: macOS tints it for light/dark menu bars
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
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

    let _ = AUTO_STOP_ITEMS.set(AutoStopItems {
        toggle: auto_toggle,
        status: auto_status,
        keep: auto_keep,
    });
    let _ = TIMED_ITEMS.set(TimedItems {
        start: start_item,
        status: time_status,
        extend: time_extend,
        no_limit: time_remove,
    });

    log::info!("✅ System tray created with context menu (ID: {})", TRAY_ID);
    Ok(())
}

/// Handle tray menu events
fn handle_tray_event(app: &AppHandle, id: &str) {
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
        tray_ids::RESUME_RECORDING => {
            emit_to_frontend(app, "tray:resume_recording");
        }
        tray_ids::AUTO_STOP_TOGGLE => {
            // The check item has already flipped itself; persist its state
            let enabled = AUTO_STOP_ITEMS
                .get()
                .and_then(|i| i.toggle.is_checked().ok())
                .unwrap_or(true);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::meeting_end::set_auto_stop_settings(app, enabled, None).await {
                    log::warn!("Failed to save auto-stop setting: {}", e);
                }
            });
        }
        tray_ids::AUTO_STOP_KEEP => {
            crate::meeting_end::keep_recording(app);
        }
        // Timed recording
        id if start_duration_for(id).is_some() => {
            let duration = start_duration_for(id).unwrap_or("none");
            if let Err(e) = app.emit("tray:start_recording", serde_json::json!({ "duration": duration })) {
                log::error!("Failed to emit tray:start_recording: {}", e);
            }
        }
        tray_ids::TIME_LIMIT_EXTEND => {
            if let Err(e) = crate::timed_recording::extend(app, None) {
                log::info!("Tray +15 min: {}", e);
            }
        }
        tray_ids::TIME_LIMIT_REMOVE => {
            if let Err(e) = crate::timed_recording::remove_limit(app, None) {
                log::info!("Tray remove limit: {}", e);
            }
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
