use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Instant;

use gpui::{App, Styled, Window, WindowHandle};
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem, SubMenu};
use ksni::{Icon, ToolTip, Tray};
use notify_rust::{Hint, Notification, Timeout};
use rfd::FileDialog;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, event_created_child};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_device_v1::{
    EVT_DATA_OFFER_OPCODE as EXT_DATA_OFFER_OPCODE, Event as ExtDataControlDeviceEvent,
    ExtDataControlDeviceV1,
};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::ExtDataControlManagerV1;
use wayland_protocols::ext::data_control::v1::client::ext_data_control_offer_v1::ExtDataControlOfferV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    EVT_DATA_OFFER_OPCODE as WLR_DATA_OFFER_OPCODE, Event as ZwlrDataControlDeviceEvent,
    ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;
use wl_clipboard_rs::copy::{MimeType as CopyMimeType, Options as CopyOptions, Source};
use wl_clipboard_rs::paste::{
    ClipboardType, MimeType as PasteMimeType, Seat, get_contents, get_mime_types_ordered,
};

use crate::storage::ClipboardStorage;
use crate::{
    AutoClearState, FontChoice, LauncherExitIntent, LauncherView, MENU_COMMAND_TX, MenuCommand,
    NEURAL_STATUS, NeuralStatus, SelfClipboardWriteState, UiStyleState,
};

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

#[derive(Default)]
struct ClipboardChangeState {
    next_change_count: i64,
    last_signature: Option<String>,
}

enum ClipboardManager {
    Zwlr(ZwlrDataControlManagerV1),
    Ext(ExtDataControlManagerV1),
}

enum ClipboardDevice {
    Zwlr(ZwlrDataControlDeviceV1),
    Ext(ExtDataControlDeviceV1),
}

struct WaylandClipboardMonitorState {
    devices: Vec<ClipboardDevice>,
}

static CLIPBOARD_CHANGE_STATE: OnceLock<Mutex<ClipboardChangeState>> = OnceLock::new();
static WAYLAND_CLIPBOARD_CHANGE_COUNT: AtomicI64 = AtomicI64::new(0);
static WAYLAND_CLIPBOARD_MONITOR_START: OnceLock<()> = OnceLock::new();

pub(crate) fn clipboard_change_count() -> i64 {
    if is_wayland_session() {
        ensure_wayland_clipboard_monitor();
        return WAYLAND_CLIPBOARD_CHANGE_COUNT.load(Ordering::Acquire);
    }

    polling_clipboard_change_count()
}

fn polling_clipboard_change_count() -> i64 {
    let signature = current_clipboard_signature();
    let state = CLIPBOARD_CHANGE_STATE.get_or_init(|| Mutex::new(ClipboardChangeState::default()));
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.last_signature != signature {
        guard.next_change_count = guard.next_change_count.wrapping_add(1);
        guard.last_signature = signature;
    }
    guard.next_change_count
}

impl ClipboardManager {
    fn get_data_device(
        &self,
        seat: &WlSeat,
        qh: &wayland_client::QueueHandle<WaylandClipboardMonitorState>,
    ) -> ClipboardDevice {
        match self {
            Self::Zwlr(manager) => ClipboardDevice::Zwlr(manager.get_data_device(seat, qh, ())),
            Self::Ext(manager) => ClipboardDevice::Ext(manager.get_data_device(seat, qh, ())),
        }
    }
}

fn ensure_wayland_clipboard_monitor() {
    WAYLAND_CLIPBOARD_MONITOR_START.get_or_init(|| {
        std::thread::Builder::new()
            .name("pasta-linux-clipboard-monitor".to_owned())
            .spawn(move || {
                if let Err(err) = run_wayland_clipboard_monitor() {
                    eprintln!("warning: failed to start Wayland clipboard monitor: {err}");
                }
            })
            .unwrap_or_else(|err| {
                panic!("failed to spawn Wayland clipboard monitor thread: {err}");
            });
    });
}

fn run_wayland_clipboard_monitor() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|err| err.to_string())?;
    let (globals, mut queue) = registry_queue_init::<WaylandClipboardMonitorState>(&conn)
        .map_err(|err| err.to_string())?;
    let qh = queue.handle();

    let manager = globals
        .bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .ok()
        .map(ClipboardManager::Ext)
        .or_else(|| {
            globals
                .bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ())
                .ok()
                .map(ClipboardManager::Zwlr)
        })
        .ok_or_else(|| "missing ext-data-control / wlr-data-control protocol".to_owned())?;

    let registry = globals.registry();
    let seats: Vec<WlSeat> = globals.contents().with_list(|globals| {
        globals
            .iter()
            .filter(|global| global.interface == WlSeat::interface().name && global.version >= 2)
            .map(|global| registry.bind(global.name, 2, &qh, ()))
            .collect()
    });

    if seats.is_empty() {
        return Err("no Wayland seats available for clipboard monitor".to_owned());
    }

    let mut state = WaylandClipboardMonitorState {
        devices: seats
            .iter()
            .map(|seat| manager.get_data_device(seat, &qh))
            .collect(),
    };

    queue.roundtrip(&mut state).map_err(|err| err.to_string())?;
    loop {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|err| err.to_string())?;
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlManagerV1,
        _event: <ExtDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlOfferV1,
        _event: <ExtDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlOfferV1,
        _event: <ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlDeviceV1,
        event: <ExtDataControlDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            ExtDataControlDeviceEvent::Selection { .. }
            | ExtDataControlDeviceEvent::PrimarySelection { .. } => {
                WAYLAND_CLIPBOARD_CHANGE_COUNT.fetch_add(1, Ordering::AcqRel);
            }
            _ => {}
        }
    }

    event_created_child!(WaylandClipboardMonitorState, ExtDataControlDeviceV1, [
        EXT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for WaylandClipboardMonitorState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlDeviceV1,
        event: <ZwlrDataControlDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            ZwlrDataControlDeviceEvent::Selection { .. }
            | ZwlrDataControlDeviceEvent::PrimarySelection { .. } => {
                WAYLAND_CLIPBOARD_CHANGE_COUNT.fetch_add(1, Ordering::AcqRel);
            }
            _ => {}
        }
    }

    event_created_child!(WaylandClipboardMonitorState, ZwlrDataControlDeviceV1, [
        WLR_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

/// SHA-256 hash of the given text, used to de-duplicate clipboard items.
pub(crate) fn clipboard_text_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn read_clipboard_snapshot() -> Option<ClipboardSnapshot> {
    let mime_types = read_clipboard_mime_types();
    let text = read_clipboard_text()?;
    Some(ClipboardSnapshot {
        text,
        is_concealed: clipboard_looks_concealed(&mime_types),
        is_transient: clipboard_looks_transient(&mime_types),
    })
}

/// Returns true if we should ignore this clipboard write because we
/// ourselves just wrote it.
pub(crate) fn should_ignore_self_clipboard_write(cx: &mut App, text: &str) -> bool {
    let pending = cx
        .try_global::<SelfClipboardWriteState>()
        .and_then(|state| state.pending.clone());
    let Some(pending) = pending else { return false };

    if Instant::now() > pending.due_at {
        cx.global_mut::<SelfClipboardWriteState>().pending = None;
        return false;
    }

    if clipboard_text_hash(text) == pending.expected_hash {
        cx.global_mut::<SelfClipboardWriteState>().pending = None;
        return true;
    }

    false
}

/// Process secret auto-clear timer.
pub(crate) fn process_secret_autoclear(cx: &mut App) {
    let pending = cx
        .try_global::<AutoClearState>()
        .and_then(|state| state.pending.clone());
    let Some(pending) = pending else { return };
    if Instant::now() < pending.due_at {
        return;
    }

    let should_clear = read_clipboard_text()
        .map(|current| clipboard_text_hash(&current) == pending.expected_hash)
        .unwrap_or(false);
    if should_clear {
        write_clipboard_text("");
    }

    cx.global_mut::<AutoClearState>().pending = None;
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
    if Notification::new()
        .summary(title)
        .body(body)
        .appname("Pasta")
        .hint(Hint::Transient(true))
        .timeout(Timeout::Milliseconds(2_500))
        .show()
        .is_err()
    {
        eprintln!("[notification] {title}: {body}");
    }
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
        let (mut pipe, _) = get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            PasteMimeType::Text,
        )
        .ok()?;
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

pub(crate) struct StatusItemRegistration {
    _handle: ksni::blocking::Handle<PastaTray>,
}

impl gpui::Global for StatusItemRegistration {}

struct PastaTray {
    menu_tx: mpsc::Sender<MenuCommand>,
    font_choice: FontChoice,
    syntax_highlighting: bool,
    secret_auto_clear: bool,
    pasta_brain_enabled: bool,
    neural_status: NeuralStatus,
}

impl PastaTray {
    fn sync_from_app(&mut self, style: &UiStyleState, neural_status: NeuralStatus) {
        self.font_choice = font_choice_from_family(&style.family);
        self.syntax_highlighting = style.syntax_highlighting;
        self.secret_auto_clear = style.secret_auto_clear;
        self.pasta_brain_enabled = style.pasta_brain_enabled;
        self.neural_status = neural_status;
    }
}

impl Tray for PastaTray {
    fn id(&self) -> String {
        "pasta".into()
    }

    fn title(&self) -> String {
        "Pasta".into()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![pasta_tray_icon()]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: vec![pasta_tray_icon()],
            title: "Pasta".into(),
            description: "Clipboard manager for devs and devops".into(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.menu_tx.send(MenuCommand::ShowLauncher);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        items.push(
            StandardItem {
                label: "Show Pasta".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.menu_tx.send(MenuCommand::ShowLauncher);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);

        items.push(
            StandardItem {
                label: "About Pasta".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.menu_tx.send(MenuCommand::ShowAbout);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);

        items.push(
            SubMenu {
                label: "Font".into(),
                submenu: FontChoice::ALL
                    .into_iter()
                    .map(|choice| {
                        CheckmarkItem {
                            label: choice.label().into(),
                            checked: choice == self.font_choice,
                            activate: Box::new(move |tray: &mut Self| {
                                tray.font_choice = choice;
                                let _ = tray.menu_tx.send(MenuCommand::SetFont(choice));
                            }),
                            ..Default::default()
                        }
                        .into()
                    })
                    .collect(),
                ..Default::default()
            }
            .into(),
        );

        items.push(
            SubMenu {
                label: "Syntax Highlighting".into(),
                submenu: vec![
                    CheckmarkItem {
                        label: "Enabled".into(),
                        checked: self.syntax_highlighting,
                        activate: Box::new(|tray: &mut Self| {
                            tray.syntax_highlighting = true;
                            let _ = tray.menu_tx.send(MenuCommand::SetSyntaxHighlighting(true));
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Disabled".into(),
                        checked: !self.syntax_highlighting,
                        activate: Box::new(|tray: &mut Self| {
                            tray.syntax_highlighting = false;
                            let _ = tray.menu_tx.send(MenuCommand::SetSyntaxHighlighting(false));
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
        );

        items.push(
            SubMenu {
                label: "Secret Copy Auto-Clear".into(),
                submenu: vec![
                    CheckmarkItem {
                        label: "Enabled (30s)".into(),
                        checked: self.secret_auto_clear,
                        activate: Box::new(|tray: &mut Self| {
                            tray.secret_auto_clear = true;
                            let _ = tray.menu_tx.send(MenuCommand::SetSecretAutoClear(true));
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Disabled".into(),
                        checked: !self.secret_auto_clear,
                        activate: Box::new(|tray: &mut Self| {
                            tray.secret_auto_clear = false;
                            let _ = tray.menu_tx.send(MenuCommand::SetSecretAutoClear(false));
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
        );

        items.push(
            SubMenu {
                label: "Pasta Brain".into(),
                submenu: vec![
                    CheckmarkItem {
                        label: "Enabled".into(),
                        checked: self.pasta_brain_enabled,
                        activate: Box::new(|tray: &mut Self| {
                            tray.pasta_brain_enabled = true;
                            let _ = tray.menu_tx.send(MenuCommand::SetPastaBrain(true));
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Disabled".into(),
                        checked: !self.pasta_brain_enabled,
                        activate: Box::new(|tray: &mut Self| {
                            tray.pasta_brain_enabled = false;
                            let _ = tray.menu_tx.send(MenuCommand::SetPastaBrain(false));
                        }),
                        ..Default::default()
                    }
                    .into(),
                    MenuItem::Separator,
                    StandardItem {
                        label: neural_download_label(self.neural_status).into(),
                        activate: Box::new(|tray: &mut Self| {
                            let _ = tray.menu_tx.send(MenuCommand::DownloadBrain);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit Pasta".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.menu_tx.send(MenuCommand::QuitApp);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        eprintln!("warning: Linux status item unavailable, continuing without tray: {reason:?}");
        true
    }
}

/// Configure the app as a background/accessory process. No-op on Linux.
pub(crate) fn configure_background_mode() {
    // On macOS this sets NSApplicationActivationPolicyAccessory.
    // On Linux, background mode is the default — no action needed.
}

pub(crate) fn setup_status_item(cx: &mut App) {
    let Some(menu_tx) = MENU_COMMAND_TX.get().cloned() else {
        eprintln!("warning: status item unavailable: menu command channel not initialized");
        return;
    };

    let style = cx.global::<UiStyleState>().clone();
    let neural_status = NEURAL_STATUS
        .lock()
        .map(|status| *status)
        .unwrap_or(NeuralStatus::Failed);

    let tray = PastaTray {
        menu_tx,
        font_choice: font_choice_from_family(&style.family),
        syntax_highlighting: style.syntax_highlighting,
        secret_auto_clear: style.secret_auto_clear,
        pasta_brain_enabled: style.pasta_brain_enabled,
        neural_status,
    };

    match tray.assume_sni_available(true).spawn() {
        Ok(handle) => {
            eprintln!("info: Linux status item initialized");
            cx.set_global(StatusItemRegistration { _handle: handle });
        }
        Err(err) => {
            eprintln!("warning: failed to initialize Linux status item: {err:?}");
        }
    }
}

/// Update the brain menu item state. Stub is a no-op.
pub(crate) fn update_brain_menu_state(cx: &App) {
    let Some(registration) = cx.try_global::<StatusItemRegistration>() else {
        return;
    };

    let style = cx.global::<UiStyleState>().clone();
    let neural_status = NEURAL_STATUS
        .lock()
        .map(|status| *status)
        .unwrap_or(NeuralStatus::Failed);

    let _ = registration._handle.update(|tray| {
        tray.sync_from_app(&style, neural_status);
    });
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
            Some(MenuCommand::SetFont(
                FontChoice::ALL[(t - MENU_TAG_FONT_BASE) as usize],
            ))
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

pub(crate) struct BackgroundAnchorView;

impl gpui::Render for BackgroundAnchorView {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div().size_full()
    }
}

pub(crate) fn create_background_anchor_window(
    cx: &mut App,
) -> Option<WindowHandle<BackgroundAnchorView>> {
    use gpui::*;

    let display_id = cx.primary_display().map(|display| display.id());
    let bounds = Bounds::centered(display_id, size(px(1.0), px(1.0)), cx);

    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            window_background: WindowBackgroundAppearance::Transparent,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_decorations: Some(WindowDecorations::Client),
            display_id,
            ..Default::default()
        },
        |_window, cx| cx.new(|_| BackgroundAnchorView),
    ) {
        Ok(window) => {
            eprintln!("info: Linux background anchor window created");
            Some(window)
        }
        Err(err) => {
            eprintln!("warning: failed to create Linux background anchor window: {err}");
            None
        }
    }
}

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
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_decorations: Some(WindowDecorations::Client),
            display_id,
            ..Default::default()
        },
        move |window, cx| {
            let storage = storage.clone();
            let style = style.clone();
            let search_tx = search_tx.clone();
            let generation_token = generation_token.clone();

            window.on_window_should_close(cx, |_, cx| {
                cx.hide();
                false
            });

            cx.new(move |cx| {
                let view = LauncherView::new(
                    storage,
                    style.family.clone(),
                    style.surface_alpha,
                    style.syntax_highlighting,
                    style.pasta_brain_enabled,
                    search_tx,
                    generation_token,
                    cx,
                );

                cx.observe_window_activation(window, |view: &mut LauncherView, window, cx| {
                    if window.is_window_active() {
                        view.blur_close_armed = true;
                        return;
                    }
                    if !view.blur_close_armed {
                        return;
                    }
                    if view.blur_hide_suppressed() {
                        return;
                    }
                    view.begin_close_transition(LauncherExitIntent::Hide);
                    cx.notify();
                })
                .detach();

                cx.observe_window_appearance(window, |_view: &mut LauncherView, _window, cx| {
                    cx.notify();
                })
                .detach();

                cx.observe_keystrokes(|view: &mut LauncherView, event, _window, cx| {
                    view.handle_keystroke(event, cx);
                })
                .detach();

                view
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

fn current_clipboard_signature() -> Option<String> {
    let mime_types = read_clipboard_mime_types();
    let mime_signature = if mime_types.is_empty() {
        "mime:none".to_owned()
    } else {
        format!("mime:{}", mime_types.join("|"))
    };

    let text_signature = read_clipboard_text()
        .map(|text| format!("text:{}", clipboard_text_hash(&text)))
        .unwrap_or_else(|| "text:none".to_owned());

    Some(format!("{mime_signature};{text_signature}"))
}

fn read_clipboard_mime_types() -> Vec<String> {
    if is_wayland_session() {
        return get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
            .unwrap_or_default();
    }

    if command_exists("xclip") {
        return read_via_command("xclip", &["-selection", "clipboard", "-t", "TARGETS", "-o"])
            .map(|output| {
                output
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
    }

    Vec::new()
}

fn clipboard_looks_concealed(mime_types: &[String]) -> bool {
    mime_types.iter().any(|mime| {
        let lowered = mime.to_ascii_lowercase();
        lowered.contains("concealed")
            || lowered.contains("secret")
            || lowered.contains("password")
            || lowered.contains("onepassword")
            || lowered.contains("bitwarden")
            || lowered.contains("keepass")
    })
}

fn clipboard_looks_transient(mime_types: &[String]) -> bool {
    mime_types.iter().any(|mime| {
        let lowered = mime.to_ascii_lowercase();
        lowered.contains("transient")
            || lowered.contains("x-kde-passwordmanagerhint")
            || lowered.contains("application/x-gtk-text-buffer-rich-text")
    })
}

fn font_choice_from_family(family: &str) -> FontChoice {
    for choice in FontChoice::ALL {
        if choice
            .candidates()
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(family))
            || choice.label().eq_ignore_ascii_case(family)
        {
            return choice;
        }
    }
    FontChoice::MesloLg
}

fn neural_download_label(status: NeuralStatus) -> &'static str {
    match status {
        NeuralStatus::Loading => "Downloading Model...",
        NeuralStatus::Ready => "Model Ready ✓",
        NeuralStatus::Failed => "Download Model (Retry)",
    }
}

fn pasta_tray_icon() -> Icon {
    const WIDTH: i32 = 16;
    const HEIGHT: i32 = 16;
    const GLYPH: [&str; 16] = [
        "................",
        ".##########.....",
        ".##......##.....",
        ".##......##.....",
        ".##......##.....",
        ".#########......",
        ".##.............",
        ".##.............",
        ".##.............",
        ".##.............",
        ".##.............",
        ".##.............",
        ".##.............",
        "................",
        "................",
        "................",
    ];

    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in GLYPH {
        for pixel in row.as_bytes() {
            if *pixel == b'#' {
                data.extend_from_slice(&[0xFF, 0xF5, 0xF5, 0xF5]);
            } else {
                data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            }
        }
    }

    Icon {
        width: WIDTH,
        height: HEIGHT,
        data,
    }
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
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
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

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "missing stdin pipe".to_owned())?;
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
