// This entire module is only compiled on macOS (gated by platform/mod.rs).
// No #[cfg(target_os = "macos")] needed on individual items here.

mod clipboard;
mod files;
mod hotkey;
mod launch_agent;
mod menu;
mod style;
mod touch_id;
mod window;

pub(crate) use clipboard::{
    clipboard_change_count, clipboard_text_hash, parse_custom_tags_input, process_secret_autoclear,
    read_clipboard_snapshot, should_ignore_self_clipboard_write, show_macos_notification,
};
pub(crate) use files::{choose_bowl_export_path, choose_bowl_import_path};
pub(crate) use hotkey::setup_hotkey;
pub(crate) use launch_agent::ensure_launch_agent_registered;
#[cfg(test)]
pub(crate) use menu::menu_command_from_tag;
pub(crate) use menu::{configure_background_mode, setup_status_item, update_brain_menu_state};
pub(crate) use style::{
    apply_style_to_open_window, load_embedded_ui_font, persist_ui_style_state, resolve_font_family,
};
pub(crate) use touch_id::authenticate_with_touch_id;
pub(crate) use window::{create_launcher_window, set_window_move_to_active_space};
