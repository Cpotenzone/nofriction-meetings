// noFriction - menu bar (tray) icon. Right-click: Start Recording · Stop ·
// Pause · Resume · Mark · Open noFriction · Settings… · Quit. Nothing else
// (docs/design/FADELL_AUDIT.md F-22): the time limit, +15 min and the
// "seems to have ended" countdown live in the window's banner.

use once_cell::sync::OnceCell;
use tauri::{
    menu::{MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, Wry,
};

/// The Start Recording item: its label shows the remembered type and
/// length ("Start Recording (Meeting, 60 min)"; `timed_recording::start_label`).
static START_ITEM: OnceCell<MenuItem<Wry>> = OnceCell::new();

pub fn set_start_recording_label<R: Runtime>(_app: &AppHandle<R>, what: &str) {
    if let Some(item) = START_ITEM.get() {
        let _ = item.set_text(format!("Start Recording ({})", what));
    }
}

/// Tray menu item IDs
pub mod tray_ids {
    pub const START_RECORDING: &str = "tray_start_recording";
    pub const STOP_RECORDING: &str = "tray_stop_recording";
    pub const PAUSE_RECORDING: &str = "tray_pause_recording";
    pub const RESUME_RECORDING: &str = "tray_resume_recording";
    pub const MARK_MOMENT: &str = "tray_mark_moment";
    pub const SHOW_WINDOW: &str = "tray_show_window";
    pub const OPEN_SETTINGS: &str = "tray_open_settings";
}

/// Build the menu bar icon with its right-click menu
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    const TRAY_ID: &str = "nofriction-main-tray";

    if app.tray_by_id(TRAY_ID).is_some() {
        log::warn!("System tray already exists, skipping creation");
        return Ok(());
    }

    let start_item = MenuItemBuilder::with_id(tray_ids::START_RECORDING, "Start Recording").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&start_item)
        .item(&MenuItemBuilder::with_id(tray_ids::STOP_RECORDING, "Stop").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::PAUSE_RECORDING, "Pause").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::RESUME_RECORDING, "Resume").build(app)?)
        .item(&MenuItemBuilder::with_id(tray_ids::MARK_MOMENT, "Mark").build(app)?)
        .separator()
        .item(
            &MenuItemBuilder::with_id(tray_ids::SHOW_WINDOW, "Open noFriction")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(tray_ids::OPEN_SETTINGS, "Settings…").build(app)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("Quit noFriction"))?)
        .build()?;

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
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    let _ = START_ITEM.set(start_item);
    log::info!("System tray created (ID: {})", TRAY_ID);
    Ok(())
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Handle tray menu events
fn handle_tray_event(app: &AppHandle, id: &str) {
    log::info!("Tray menu event: {}", id);
    match id {
        tray_ids::START_RECORDING => emit_to_frontend(app, "tray:start_recording"),
        tray_ids::STOP_RECORDING => emit_to_frontend(app, "tray:stop_recording"),
        tray_ids::PAUSE_RECORDING => emit_to_frontend(app, "tray:pause_recording"),
        tray_ids::RESUME_RECORDING => emit_to_frontend(app, "tray:resume_recording"),
        tray_ids::MARK_MOMENT => crate::markers::commands::mark_from_shortcut(app),
        tray_ids::SHOW_WINDOW => show_window(app),
        tray_ids::OPEN_SETTINGS => {
            emit_to_frontend(app, "menu:settings");
            show_window(app);
        }
        _ => log::debug!("Unhandled tray event: {}", id),
    }
}

fn emit_to_frontend<R: Runtime>(app: &AppHandle<R>, event: &str) {
    if let Err(e) = app.emit(event, ()) {
        log::error!("Failed to emit {}: {}", event, e);
    }
}
