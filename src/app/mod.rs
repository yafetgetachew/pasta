#[cfg(target_os = "macos")]
mod actions;
#[cfg(target_os = "macos")]
mod runtime;
#[cfg(target_os = "macos")]
mod state;
#[cfg(target_os = "macos")]
mod view;

#[cfg(target_os = "macos")]
pub(crate) use runtime::{
    AutoClearState, HotkeyRegistration, LauncherState, PendingAutoClear, PendingSelfClipboardWrite,
    SelfClipboardWriteState, StatusItemRegistration, spawn_clipboard_watcher,
    spawn_hotkey_listener, spawn_launcher_transition_loop, spawn_menu_command_listener,
};
#[cfg(target_os = "macos")]
pub(crate) use state::LauncherView;
