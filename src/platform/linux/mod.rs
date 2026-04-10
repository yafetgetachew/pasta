use std::io::{Read, Write};
use std::path::PathBuf;

use gpui::{App, Window, WindowHandle};
use rfd::FileDialog;
use wl_clipboard_rs::copy::{MimeType as CopyMimeType, Options as CopyOptions, Source};
use wl_clipboard_rs::paste::{ClipboardType, MimeType as PasteMimeType, Seat, get_contents};

use crate::storage::ClipboardStorage;
use crate::{LauncherView, UiStyleState, FontChoice};

// ---------------------------------------------------------------------------
// Clipboard (Phase 1)
// ---------------------------------------------------------------------------

/// Snapshot of a clipboard read.
#[derive(Clone, Debug)]
pub(crate) struct ClipboardSnapshot {
    pub text: String,
    pub is_concealed: bool,
    pub is_transient: bool,
}

pub(crate) fn clipboard_change_count() -> i64 {
    0
}

/// SHA-256 hash of the given text, used to de-duplicate clipboard items.
pub(crate) fn clipboard_text_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn read_clipboard_snapshot() -> Option<ClipboardSnapshot> {
    let text = read_clipboard_text()?;
    Some(ClipboardSnapshot {
        text,
        is_concealed: false,
        is_transient: false,
    })
}

/// Returns true if we should ignore this clipboard write because we
/// ourselves just wrote it. Stub returns false.
pub(crate) fn should_ignore_self_clipboard_write(_cx: &mut App, _text: &str) -> bool {
    false
}

/// Process secret auto-clear timer. Stub is a no-op.
pub(crate) fn process_secret_autoclear(_cx: &mut App) {
    // no-op
}

/// Parse a comma-separated tag input string into a list of tags.
pub(crate) fn parse_custom_tags_input(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn show_macos_notification(title: &str, body: &str) {
    eprintln!("[notification] {title}: {body}");
}

pub(crate) fn write_clipboard_text(value: &str) {
    if is_wayland_session() {
        let options = CopyOptions::new();
        if let Err(err) = options.copy(
            Source::Bytes(value.as_bytes().to_vec().into_boxed_slice()),
            CopyMimeType::Text,
        ) {
            eprintln!("warning: failed to copy to Wayland clipboard: {err}");
        }
        return;
    }

    if command_exists("xclip") {
        if let Err(err) = write_via_command("xclip", &["-selection", "clipboard"], value) {
            eprintln!("warning: failed to copy to clipboard with xclip: {err}");
        }
        return;
    }

    if command_exists("xsel") {
        if let Err(err) = write_via_command("xsel", &["--clipboard", "--input"], value) {
            eprintln!("warning: failed to copy to clipboard with xsel: {err}");
        }
        return;
    }

    eprintln!("warning: no supported Linux clipboard backend found");
}

pub(crate) fn read_clipboard_text() -> Option<String> {
    if is_wayland_session() {
        let (mut pipe, _) = get_contents(ClipboardType::Regular, Seat::Unspecified, PasteMimeType::Text).ok()?;
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).ok()?;
        return String::from_utf8(bytes).ok();
    }

    if command_exists("xclip") {
        return read_via_command("xclip", &["-selection", "clipboard", "-o"]);
    }

    if command_exists("xsel") {
        return read_via_command("xsel", &["--clipboard", "--output"]);
    }

    None
}

// ---------------------------------------------------------------------------
// File dialogs (Phase 3)
// ---------------------------------------------------------------------------

pub(crate) fn choose_bowl_export_path(_prompt: &str, _default_name: &str) -> Option<PathBuf> {
    let mut path = FileDialog::new()
        .set_title(_prompt)
        .set_file_name(_default_name)
        .add_filter("YAML", &["yaml", "yml"])
        .save_file()?;
    if path.extension().is_none() {
        path.set_extension("yaml");
    }
    Some(path)
}

pub(crate) fn choose_bowl_import_path(_prompt: &str) -> Option<PathBuf> {
    FileDialog::new()
        .set_title(_prompt)
        .add_filter("YAML", &["yaml", "yml"])
        .pick_file()
}

// ---------------------------------------------------------------------------
// Hotkey (Phase 2)
// ---------------------------------------------------------------------------

pub(crate) fn setup_hotkey(_cx: &mut App) {
    // Registration happens in the Linux runtime listener.
}

// ---------------------------------------------------------------------------
// Autostart (Phase 3) — replaces launch_agent on Linux
// ---------------------------------------------------------------------------

/// Ensure the app is registered for autostart. Stub is a no-op.
pub(crate) fn ensure_launch_agent_registered() {
    // On Linux this will write an XDG autostart .desktop file.
    // Stub: no-op.
}

// ---------------------------------------------------------------------------
// System tray / menu (Phase 2)
// ---------------------------------------------------------------------------

/// Configure the app as a background/accessory process. No-op on Linux.
pub(crate) fn configure_background_mode() {
    // On macOS this sets NSApplicationActivationPolicyAccessory.
    // On Linux, background mode is the default — no action needed.
}

pub(crate) fn setup_status_item(_cx: &mut App) {
    eprintln!("info: system tray not yet implemented on Linux (stub)");
}

/// Update the brain menu item state. Stub is a no-op.
pub(crate) fn update_brain_menu_state(_cx: &App) {
    // no-op
}

/// Map a menu tag integer to a MenuCommand. Stub for tests.
#[cfg(test)]
pub(crate) fn menu_command_from_tag(tag: isize) -> Option<crate::MenuCommand> {
    use crate::*;
    match tag {
        MENU_TAG_SHOW => Some(MenuCommand::ShowLauncher),
        MENU_TAG_QUIT => Some(MenuCommand::QuitApp),
        MENU_TAG_ABOUT => Some(MenuCommand::ShowAbout),
        MENU_TAG_SYNTAX_ON => Some(MenuCommand::SetSyntaxHighlighting(true)),
        MENU_TAG_SYNTAX_OFF => Some(MenuCommand::SetSyntaxHighlighting(false)),
        MENU_TAG_SECRET_CLEAR_ON => Some(MenuCommand::SetSecretAutoClear(true)),
        MENU_TAG_SECRET_CLEAR_OFF => Some(MenuCommand::SetSecretAutoClear(false)),
        MENU_TAG_BRAIN_ON => Some(MenuCommand::SetPastaBrain(true)),
        MENU_TAG_BRAIN_OFF => Some(MenuCommand::SetPastaBrain(false)),
        MENU_TAG_BRAIN_DOWNLOAD => Some(MenuCommand::DownloadBrain),
        t if t >= MENU_TAG_FONT_BASE && t < MENU_TAG_FONT_BASE + FontChoice::ALL.len() as isize => {
            Some(MenuCommand::SetFont(FontChoice::ALL[(t - MENU_TAG_FONT_BASE) as usize]))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Style & Fonts (Phase 4)
// ---------------------------------------------------------------------------

pub(crate) fn load_embedded_ui_font(_cx: &mut App) {
    // Fonts are embedded via include_bytes! — same approach works on Linux.
    // Stub: no-op until Phase 4.
}

/// Resolve the user's font choice to an actual font family name.
pub(crate) fn resolve_font_family(_cx: &App, _choice: FontChoice) -> Option<gpui::SharedString> {
    // Default to the first embedded font, matching macOS behavior.
    Some("Meslo LG".into())
}

/// Apply the current style state to an open window. Stub is a no-op.
pub(crate) fn apply_style_to_open_window(_cx: &mut App) {
    // no-op
}

/// Persist the current UI style state to disk. Stub is a no-op.
pub(crate) fn persist_ui_style_state(_cx: &App) {
    // no-op
}

// ---------------------------------------------------------------------------
// Touch ID / Auth (Phase 3)
// ---------------------------------------------------------------------------

/// Authenticate the user (Touch ID on macOS). Stub always returns true.
pub(crate) fn authenticate_with_touch_id(_reason: &str) -> bool {
    // On Linux, skip biometric auth for MVP. Always grant access.
    true
}

// ---------------------------------------------------------------------------
// Window (Phase 4)
// ---------------------------------------------------------------------------

/// Create the main launcher window. Stub creates a basic GPUI window.
pub(crate) fn create_launcher_window(cx: &mut App) -> Option<WindowHandle<LauncherView>> {
    use gpui::*;

    let display_id = cx.primary_display().map(|display| display.id());
    let bounds = Bounds::centered(display_id, size(px(860.0), px(560.0)), cx);
    let storage = cx.global::<crate::StorageState>().storage.clone();
    let style = cx.global::<UiStyleState>().clone();
    let (search_tx, search_rx, generation_token) =
        crate::app::state::start_search_worker(storage.clone());

    let window = match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: false,
            kind: WindowKind::Normal,
            window_background: WindowBackgroundAppearance::Opaque,
            window_decorations: Some(WindowDecorations::Client),
            display_id,
            ..Default::default()
        },
        |_window, cx| {
            cx.new(|cx| {
                LauncherView::new(
                    storage,
                    style.family.clone(),
                    style.surface_alpha,
                    style.syntax_highlighting,
                    style.pasta_brain_enabled,
                    search_tx,
                    generation_token,
                    cx,
                )
            })
        },
    ) {
        Ok(window) => {
            eprintln!("info: Linux launcher window created");
            window
        }
        Err(err) => {
            eprintln!("warning: failed to open Linux launcher window: {err}");
            return None;
        }
    };

    crate::app::spawn_search_result_listener(cx, window, search_rx);
    Some(window)
}

/// Set the window to move to the active workspace/space. No-op on Wayland.
pub(crate) fn set_window_move_to_active_space(_window: &Window) {
    // On Wayland, the compositor controls workspace placement.
    // Hyprland window rules handle this via config, not code.
}

fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn command_exists(program: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {program} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn read_via_command(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn write_via_command(program: &str, args: &[&str], value: &str) -> Result<(), String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let mut stdin = child.stdin.take().ok_or_else(|| "missing stdin pipe".to_owned())?;
    stdin
        .write_all(value.as_bytes())
        .map_err(|err| err.to_string())?;
    drop(stdin);

    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with status {status}"))
    }
}
