// noFriction Meetings - Native macOS Menu Builder
// Creates native menu bar following Apple Human Interface Guidelines

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Runtime, Wry,
};

/// Menu item IDs for event handling
pub mod menu_ids {
    pub const NEW_RECORDING: &str = "new_recording";
    pub const STOP_RECORDING: &str = "stop_recording";
    pub const EXPORT_MEETING: &str = "export_meeting";
    pub const IMPORT_PROMPTS: &str = "import_prompts";
    pub const EXPORT_PROMPTS: &str = "export_prompts";

    pub const SUMMARIZE: &str = "summarize_meeting";
    pub const ACTION_ITEMS: &str = "extract_action_items";
    pub const ASK_AI: &str = "ask_ai";
    pub const ANALYZE_FRAMES: &str = "analyze_frames";

    pub const VIEW_LIVE: &str = "view_live";
    pub const VIEW_REWIND: &str = "view_rewind";
    pub const VIEW_SETTINGS: &str = "view_settings";
    pub const VIEW_PROMPTS: &str = "view_prompts";
    pub const COMMAND_PALETTE: &str = "command_palette";

    pub const REFRESH_MODELS: &str = "refresh_models";
    pub const CLEAR_CACHE: &str = "clear_cache";

    // Always-On Capture Mode (v2.5.0)
    pub const MODE_AMBIENT: &str = "mode_ambient";
    pub const MODE_MEETING: &str = "mode_meeting";
    pub const MODE_PAUSE: &str = "mode_pause";
}

/// Build the application menu bar
pub fn create_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // App menu (noFriction Meetings)
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
        .separator()
        .item(
            &MenuItemBuilder::with_id(menu_ids::EXPORT_MEETING, "Export Meeting...")
                .accelerator("CmdOrCtrl+Shift+E")
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id(menu_ids::IMPORT_PROMPTS, "Import Prompts...").build(app)?)
        .item(&MenuItemBuilder::with_id(menu_ids::EXPORT_PROMPTS, "Export Prompts...").build(app)?)
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
            &MenuItemBuilder::with_id(menu_ids::VIEW_LIVE, "Live View")
                .accelerator("CmdOrCtrl+1")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_REWIND, "Rewind View")
                .accelerator("CmdOrCtrl+2")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id(menu_ids::VIEW_PROMPTS, "Prompt Library")
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

    // Meeting menu
    let meeting_menu = SubmenuBuilder::new(app, "Meeting")
        .item(
            &MenuItemBuilder::with_id(menu_ids::SUMMARIZE, "Summarize")
                .accelerator("CmdOrCtrl+Shift+S")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::ACTION_ITEMS, "Extract Action Items")
                .accelerator("CmdOrCtrl+Shift+A")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id(menu_ids::ASK_AI, "Ask AI...")
                .accelerator("CmdOrCtrl+Shift+I")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id(menu_ids::ANALYZE_FRAMES, "Analyze Screen Captures")
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id(menu_ids::REFRESH_MODELS, "Refresh AI Models").build(app)?)
        .separator()
        // Always-On Recording Modes
        .item(&MenuItemBuilder::with_id(menu_ids::MODE_AMBIENT, "🌙 Ambient Mode").build(app)?)
        .item(&MenuItemBuilder::with_id(menu_ids::MODE_MEETING, "🎙️ Meeting Mode").build(app)?)
        .item(&MenuItemBuilder::with_id(menu_ids::MODE_PAUSE, "⏸️ Pause Recording").build(app)?)
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
        .item(&MenuItemBuilder::new("noFriction Meetings Help").build(app)?)
        .separator()
        .item(&MenuItemBuilder::new("Keyboard Shortcuts").build(app)?)
        .item(&MenuItemBuilder::new("Report an Issue...").build(app)?)
        .build()?;

    // Build the complete menu bar
    MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&meeting_menu)
        .item(&window_menu)
        .item(&help_menu)
        .build()
}

/// Handle menu item events
pub fn handle_menu_event(app: &AppHandle<Wry>, event_id: &str) {
    match event_id {
        menu_ids::NEW_RECORDING => {
            emit_to_frontend(app, "menu:new_recording");
        }
        menu_ids::STOP_RECORDING => {
            emit_to_frontend(app, "menu:stop_recording");
        }
        menu_ids::EXPORT_MEETING => {
            emit_to_frontend(app, "menu:export_meeting");
        }
        menu_ids::SUMMARIZE => {
            emit_to_frontend(app, "menu:summarize");
        }
        menu_ids::ACTION_ITEMS => {
            emit_to_frontend(app, "menu:action_items");
        }
        menu_ids::ASK_AI => {
            emit_to_frontend(app, "menu:ask_ai");
        }
        menu_ids::ANALYZE_FRAMES => {
            emit_to_frontend(app, "menu:analyze_frames");
        }
        menu_ids::VIEW_LIVE => {
            emit_to_frontend(app, "menu:view_live");
        }
        menu_ids::VIEW_REWIND => {
            emit_to_frontend(app, "menu:view_rewind");
        }
        menu_ids::VIEW_SETTINGS => {
            emit_to_frontend(app, "menu:view_settings");
        }
        menu_ids::VIEW_PROMPTS => {
            emit_to_frontend(app, "menu:view_prompts");
        }
        menu_ids::COMMAND_PALETTE => {
            emit_to_frontend(app, "menu:command_palette");
        }
        menu_ids::REFRESH_MODELS => {
            emit_to_frontend(app, "menu:refresh_models");
        }
        menu_ids::CLEAR_CACHE => {
            emit_to_frontend(app, "menu:clear_cache");
        }
        menu_ids::IMPORT_PROMPTS => {
            emit_to_frontend(app, "menu:import_prompts");
        }
        menu_ids::EXPORT_PROMPTS => {
            emit_to_frontend(app, "menu:export_prompts");
        }
        // Always-On Capture Modes
        menu_ids::MODE_AMBIENT => {
            emit_to_frontend(app, "menu:mode_ambient");
        }
        menu_ids::MODE_MEETING => {
            emit_to_frontend(app, "menu:mode_meeting");
        }
        menu_ids::MODE_PAUSE => {
            emit_to_frontend(app, "menu:mode_pause");
        }
        _ => {
            log::debug!("Unhandled menu event: {}", event_id);
        }
    }
}

fn emit_to_frontend(app: &AppHandle<Wry>, event: &str) {
    if let Err(e) = app.emit(event, ()) {
        log::error!("Failed to emit menu event {}: {}", event, e);
    }
}
