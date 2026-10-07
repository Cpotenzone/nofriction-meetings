// noFriction Meetings - Native macOS Menu Builder
// Creates native menu bar following Apple Human Interface Guidelines

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Runtime, Wry,
};

/// Menu item IDs for event handling. Every item in the menu bar does
/// something: the frontend listens for the `menu:*` event each one emits.
pub mod menu_ids {
    pub const NEW_RECORDING: &str = "new_recording";
    pub const STOP_RECORDING: &str = "stop_recording";
    pub const PAUSE_RECORDING: &str = "mode_pause";
    /// ★ the current moment of the recording (markers.rs)
    pub const MARK_MOMENT: &str = "mark_moment";

    pub const ASK_AI: &str = "ask_ai";

    pub const VIEW_LIVE: &str = "view_live";
    pub const VIEW_REWIND: &str = "view_rewind";
    pub const VIEW_SETTINGS: &str = "view_settings";
    pub const VIEW_PROMPTS: &str = "view_prompts";
    pub const COMMAND_PALETTE: &str = "command_palette";

    pub const HELP: &str = "help";
    pub const CONTACT_SUPPORT: &str = "contact_support";
}

/// Support page (keep in sync with SUPPORT_URL in src/lib/build.ts).
pub const SUPPORT_URL: &str = "https://nofriction.io/contact";

/// Build the application menu bar
pub fn create_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = SubmenuBuilder::new(app, "noFriction Meetings")
        .item(&PredefinedMenuItem::about(
            app,
            Some("About noFriction Meetings"),
            None,
        )?)
        .separator()
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_SETTINGS, "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, None)?)
        .build()?;

    // File menu
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id(menu_ids::NEW_RECORDING, "New Recording")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::STOP_RECORDING, "Stop Recording")
                .accelerator("CmdOrCtrl+.")
                .build(app)?,
        )
        .item(&MenuItemBuilder::with_id(menu_ids::PAUSE_RECORDING, "Pause Recording").build(app)?)
        .item(
            &MenuItemBuilder::with_id(menu_ids::MARK_MOMENT, "Mark Moment")
                .accelerator(crate::markers::MENU_ACCELERATOR)
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    // Edit menu
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    // View menu
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_LIVE, "Live")
                .accelerator("CmdOrCtrl+1")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_REWIND, "Recordings")
                .accelerator("CmdOrCtrl+2")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::ASK_AI, "Chat with Your Recordings")
                .accelerator("CmdOrCtrl+Shift+I")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_PROMPTS, "Prompts")
                .accelerator("CmdOrCtrl+Shift+P")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id(menu_ids::COMMAND_PALETTE, "Command Palette...")
                .accelerator("CmdOrCtrl+K")
                .build(app)?,
        )
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    // Window menu
    let window_menu = SubmenuBuilder::new(app, "Window")
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, None)?)
        .build()?;

    // Help menu
    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&MenuItemBuilder::with_id(menu_ids::HELP, "noFriction Meetings Help").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(menu_ids::CONTACT_SUPPORT, "Contact Support...").build(app)?)
        .build()?;

    // Build the complete menu bar
    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&window_menu)
        .item(&help_menu)
        .build()
}

/// Handle menu item events (app menu bar; the tray has its own handler)
pub fn handle_menu_event(app: &AppHandle<Wry>, event_id: &str) {
    let event = match event_id {
        menu_ids::NEW_RECORDING => "menu:new_recording",
        menu_ids::STOP_RECORDING => "menu:stop_recording",
        menu_ids::PAUSE_RECORDING => "menu:mode_pause",
        menu_ids::MARK_MOMENT => {
            crate::markers::commands::mark_from_shortcut(app);
            return;
        }
        menu_ids::ASK_AI => "menu:ask_ai",
        menu_ids::VIEW_LIVE => "menu:view_live",
        menu_ids::VIEW_REWIND => "menu:view_rewind",
        menu_ids::VIEW_SETTINGS => "menu:view_settings",
        menu_ids::VIEW_PROMPTS => "menu:view_prompts",
        menu_ids::COMMAND_PALETTE => "menu:command_palette",
        menu_ids::HELP => "menu:help",
        menu_ids::CONTACT_SUPPORT => {
            use tauri_plugin_opener::OpenerExt;
            if let Err(e) = app.opener().open_url(SUPPORT_URL, None::<&str>) {
                log::warn!("Could not open the support page: {}", e);
            }
            return;
        }
        _ => {
            log::debug!("Unhandled menu event: {}", event_id);
            return;
        }
    };
    emit_to_frontend(app, event);
}

fn emit_to_frontend(app: &AppHandle<Wry>, event: &str) {
    if let Err(e) = app.emit(event, ()) {
        log::error!("Failed to emit menu event {}: {}", event, e);
    }
}
