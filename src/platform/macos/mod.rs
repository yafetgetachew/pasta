#[cfg(target_os = "macos")]
mod analytics;
#[cfg(target_os = "macos")]
mod clipboard;
#[cfg(target_os = "macos")]
mod files;
#[cfg(target_os = "macos")]
mod hotkey;
#[cfg(target_os = "macos")]
mod launch_agent;
#[cfg(target_os = "macos")]
mod menu;
#[cfg(target_os = "macos")]
mod style;
#[cfg(target_os = "macos")]
mod touch_id;
#[cfg(target_os = "macos")]
mod window;

#[cfg(target_os = "macos")]
pub(crate) use analytics::{maybe_send_heartbeat, send_heartbeat_now};
#[cfg(target_os = "macos")]
pub(crate) use clipboard::{
    clipboard_change_count, clipboard_text_hash, parse_custom_tags_input, process_secret_autoclear,
    read_clipboard_snapshot, should_ignore_self_clipboard_write, show_macos_notification,
};
#[cfg(target_os = "macos")]
pub(crate) use files::{choose_bowl_export_path, choose_bowl_import_path};
#[cfg(target_os = "macos")]
pub(crate) use hotkey::setup_hotkey;
#[cfg(target_os = "macos")]
pub(crate) use launch_agent::{
    ensure_launch_agent_registered, install_launch_agent, launch_agent_is_installed,
    uninstall_launch_agent,
};
#[cfg(all(target_os = "macos", test))]
pub(crate) use menu::menu_command_from_tag;
#[cfg(target_os = "macos")]
pub(crate) use menu::{
    configure_background_mode, setup_status_item, update_analytics_menu_state,
    update_brain_menu_state, update_font_menu_state, update_launch_at_login_menu_state,
    update_secret_menu_state, update_syntax_menu_state,
};
#[cfg(target_os = "macos")]
pub(crate) use style::{
    apply_style_to_open_window, load_embedded_ui_font, persist_ui_style_state, resolve_font_family,
};
#[cfg(target_os = "macos")]
pub(crate) use touch_id::authenticate_with_touch_id;
#[cfg(target_os = "macos")]
pub(crate) use window::{create_launcher_window, set_window_move_to_active_space};
