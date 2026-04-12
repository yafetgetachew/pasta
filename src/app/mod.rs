mod actions;
mod query_input;
mod runtime;
pub(crate) mod state;
mod view;

#[cfg(target_os = "linux")]
pub(crate) use runtime::BackgroundAnchorState;
pub(crate) use runtime::{
    AutoClearState, LauncherState, PendingAutoClear, PendingSelfClipboardWrite,
    SelfClipboardWriteState, show_launcher, spawn_clipboard_watcher, spawn_hotkey_listener,
    spawn_launcher_transition_loop, spawn_menu_command_listener, spawn_search_result_listener,
};
#[cfg(target_os = "macos")]
pub(crate) use runtime::{HotkeyRegistration, StatusItemRegistration};
pub(crate) use state::{LauncherView, SearchResponse, start_search_worker};
