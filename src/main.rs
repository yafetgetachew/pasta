#![allow(unexpected_cfgs)]
#![allow(unused_imports)] // Phase 0: many imports used only by macOS or Linux platform code

mod neural_embed;
mod storage;

use std::{
    borrow::Cow,
    collections::HashSet,
    env,
    ffi::CStr,
    fs,
    ops::Range,
    path::PathBuf,
    sync::{Arc, OnceLock, mpsc},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use block::ConcreteBlock;
#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{
        NSApp, NSApplication, NSApplicationActivationPolicyAccessory, NSButton, NSMenu, NSMenuItem,
        NSPasteboard, NSPasteboardTypeString, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    },
    base::{id, nil, selector},
    foundation::NSString,
};
#[cfg(target_os = "macos")]
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::{
    App, Application, Bounds, ClickEvent, ClipboardItem, Context, CursorStyle,
    Element as GpuiElement, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, FontWeight, Global, GlobalElementId, InspectorElementId, KeyBinding,
    KeystrokeEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, Render, ScrollStrategy, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, UniformListScrollHandle, Window, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions,
    div, fill, point, prelude::*, px, relative, rgb, rgba, size, uniform_list,
};
#[cfg(target_os = "macos")]
use objc::rc::StrongPtr;
#[cfg(target_os = "macos")]
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use storage::{
    BowlExportBundle, BowlExportItem, BowlExportParameter, ClipboardItemType, ClipboardParameter,
    ClipboardRecord, ClipboardStorage, SearchQuery, bowl_name_from_tags, parse_search_query,
    render_parameterized_content, tags_without_bowl,
};

actions!(
    pasta_query_input,
    [
        QueryBackspace,
        QueryLeft,
        QueryRight,
        QuerySelectLeft,
        QuerySelectRight,
        QuerySelectAll,
        QueryHome,
        QueryEnd,
        QueryShowCharacterPalette,
        QueryPaste,
        QueryCut,
        QueryCopy,
    ]
);

#[cfg(target_os = "macos")]
#[link(name = "LocalAuthentication", kind = "framework")]
unsafe extern "C" {}

const LAUNCHER_WIDTH: f32 = 860.0;
const LAUNCHER_HEIGHT: f32 = 560.0;
const TOP_OFFSET: f32 = 146.0;
const LAUNCH_AGENT_LABEL: &str = "com.pasta.launcher";
const PREVIEW_LINE_LIMIT: usize = 4;
const PREVIEW_WRAP_RUN: usize = 96;
const WINDOW_OPEN_DURATION_MS: u64 = 120;
const WINDOW_CLOSE_DURATION_MS: u64 = 95;
const WINDOW_CLOSE_EARLY_EXIT_ALPHA: f32 = 0.08;
const MAX_VISIBLE_TAG_CHIPS: usize = 5;

const RESULT_ROW_HEIGHT: f32 = 118.0;
const RESULTS_LIST_WIDTH_RATIO: f32 = 0.47;
const PREVIEW_SETTLE_DELAY_MS: u64 = 80;
const PREVIEW_PANE_TEXT_LIMIT: usize = 24_000;
const PREVIEW_PANE_SYNTAX_MAX_CHARS: usize = 12_000;
const PREVIEW_PANE_SYNTAX_MAX_LINES: usize = 320;

#[cfg(target_os = "macos")]
const NS_WINDOW_COLLECTION_BEHAVIOR_MOVE_TO_ACTIVE_SPACE: usize = 1 << 1;

#[derive(Clone)]
struct StorageState {
    storage: Arc<ClipboardStorage>,
}

impl Global for StorageState {}

#[derive(Clone)]
pub(crate) struct UiStyleState {
    pub family: SharedString,
    pub surface_alpha: f32,
    pub syntax_highlighting: bool,
    pub secret_auto_clear: bool,
    pub pasta_brain_enabled: bool,
}

impl Global for UiStyleState {}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedUiStyleState {
    family: String,
    surface_alpha: f32,
    syntax_highlighting: bool,
    secret_auto_clear: bool,
    #[serde(default = "default_pasta_brain_enabled")]
    pasta_brain_enabled: bool,
}

fn default_pasta_brain_enabled() -> bool {
    true
}

pub(crate) const MENU_TAG_SHOW: isize = 1;
pub(crate) const MENU_TAG_QUIT: isize = 2;
pub(crate) const MENU_TAG_FONT_BASE: isize = 100;
pub(crate) const MENU_TAG_ABOUT: isize = 200;
pub(crate) const MENU_TAG_SYNTAX_ON: isize = 300;
pub(crate) const MENU_TAG_SYNTAX_OFF: isize = 301;
pub(crate) const MENU_TAG_SECRET_CLEAR_ON: isize = 302;
pub(crate) const MENU_TAG_SECRET_CLEAR_OFF: isize = 303;

pub(crate) const MENU_TAG_BRAIN_ON: isize = 304;

pub(crate) const MENU_TAG_BRAIN_OFF: isize = 305;

pub(crate) const MENU_TAG_BRAIN_DOWNLOAD: isize = 306;


static MENU_COMMAND_TX: OnceLock<mpsc::Sender<MenuCommand>> = OnceLock::new();


#[derive(Clone, Copy)]
pub(crate) enum MenuCommand {
    ShowLauncher,
    QuitApp,
    SetFont(FontChoice),
    ShowAbout,
    SetSyntaxHighlighting(bool),
    SetSecretAutoClear(bool),
    SetPastaBrain(bool),
    DownloadBrain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NeuralStatus {
    Loading,
    Ready,
    Failed,
}

pub(crate) static NEURAL_STATUS: std::sync::Mutex<NeuralStatus> =
    std::sync::Mutex::new(NeuralStatus::Loading);

#[derive(Clone, Copy)]
pub(crate) enum FontChoice {
    MesloLg,
    Iosevka,
    IbmPlexMono,
    JetBrainsMono,
    SourceCodePro,
    Monaspace,
    InputMono,
}

impl FontChoice {
    pub(crate) const ALL: [Self; 7] = [
        Self::MesloLg,
        Self::Iosevka,
        Self::IbmPlexMono,
        Self::JetBrainsMono,
        Self::SourceCodePro,
        Self::Monaspace,
        Self::InputMono,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::MesloLg => "Meslo LG",
            Self::Iosevka => "Iosevka",
            Self::IbmPlexMono => "IBM Plex Mono",
            Self::JetBrainsMono => "JetBrains Mono",
            Self::SourceCodePro => "Source Code Pro",
            Self::Monaspace => "Monaspace",
            Self::InputMono => "Input Mono",
        }
    }

    pub(crate) fn candidates(self) -> &'static [&'static str] {
        match self {
            Self::MesloLg => &[
                "MesloLGS NF",
                "MesloLGSNF-Regular",
                "Meslo LG S",
                "Meslo LG",
            ],
            Self::Iosevka => &[
                "IosevkaTermNerdFont-Light",
                "IosevkaTermNerdFont",
                "IosevkaTerm Nerd Font Mono",
                "IosevkaTerm Nerd Font",
                "Iosevka Term",
                "Iosevka",
            ],
            Self::IbmPlexMono => &["IBMPlexMono-Light", "IBMPlexMono", "IBM Plex Mono"],
            Self::JetBrainsMono => &["JetBrainsMono-Light", "JetBrainsMono", "JetBrains Mono"],
            Self::SourceCodePro => &["SourceCodePro-Var", "SourceCodePro", "Source Code Pro"],
            Self::Monaspace => &[
                "MonaspaceNeonFrozen-Light",
                "MonaspaceNeonFrozen",
                "Monaspace Neon Frozen",
                "Monaspace Neon",
                "Monaspace",
            ],
            Self::InputMono => &["Input Mono", "InputMono"],
        }
    }
}

#[derive(Clone, Copy)]
enum LauncherExitIntent {
    Hide,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagEditorMode {
    Add,
    Remove,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParameterEditorStage {
    SelectValue,
    EnterName,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextInputTarget {
    Query,
    InfoEditor,
    TagEditor,
    BowlEditor,
    ParameterName,
    ParameterFill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransformAction {
    ShellQuote,
    JsonEncode,
    JsonDecode,
    JsonPretty,
    JsonMinify,
    UrlEncode,
    UrlDecode,
    Base64Encode,
    Base64Decode,
    JwtDecode,
    EpochDecode,
    Sha256Hash,
    ContentStats,
    PublicCertPemInfo,
}

fn transform_action_for_shortcut(
    key: &str,
    modifiers: &gpui::Modifiers,
) -> Option<TransformAction> {
    let normalized_key = key.to_ascii_lowercase();
    let is_uppercase_single_char = key.len() == 1 && key.chars().all(|ch| ch.is_ascii_uppercase());
    let decode_requested = modifiers.shift || is_uppercase_single_char;

    match normalized_key.as_str() {
        "s" => Some(TransformAction::ShellQuote),
        "j" => Some(if decode_requested {
            TransformAction::JsonDecode
        } else {
            TransformAction::JsonEncode
        }),
        "f" => Some(if decode_requested {
            TransformAction::JsonMinify
        } else {
            TransformAction::JsonPretty
        }),
        "u" => Some(if decode_requested {
            TransformAction::UrlDecode
        } else {
            TransformAction::UrlEncode
        }),
        "b" => Some(if decode_requested {
            TransformAction::Base64Decode
        } else {
            TransformAction::Base64Encode
        }),
        "p" => Some(TransformAction::PublicCertPemInfo),
        "t" => Some(TransformAction::JwtDecode),
        "e" => Some(TransformAction::EpochDecode),
        "h" => Some(TransformAction::Sha256Hash),
        "c" => Some(TransformAction::ContentStats),
        _ => None,
    }
}

mod app;
mod platform;
mod transforms;
mod ui;

use app::*;
use platform::*;
use transforms::*;
use ui::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercase_shortcuts_route_to_decode_actions() {
        let no_modifiers = gpui::Modifiers::none();
        assert_eq!(
            transform_action_for_shortcut("J", &no_modifiers),
            Some(TransformAction::JsonDecode)
        );
        assert_eq!(
            transform_action_for_shortcut("U", &no_modifiers),
            Some(TransformAction::UrlDecode)
        );
        assert_eq!(
            transform_action_for_shortcut("B", &no_modifiers),
            Some(TransformAction::Base64Decode)
        );
    }

    #[test]
    fn shift_shortcuts_route_to_decode_actions() {
        let shift_modifiers = gpui::Modifiers {
            shift: true,
            ..gpui::Modifiers::none()
        };
        assert_eq!(
            transform_action_for_shortcut("j", &shift_modifiers),
            Some(TransformAction::JsonDecode)
        );
        assert_eq!(
            transform_action_for_shortcut("u", &shift_modifiers),
            Some(TransformAction::UrlDecode)
        );
        assert_eq!(
            transform_action_for_shortcut("b", &shift_modifiers),
            Some(TransformAction::Base64Decode)
        );
    }

    #[test]
    fn decode_transforms_round_trip_encoded_text() {
        let original = "kubectl get pods -n kube-system";

        let (json_encoded, _) = json_encode_transform(original).expect("json encode");
        let (json_decoded, _) = json_decode_transform(&json_encoded).expect("json decode");
        assert_eq!(json_decoded, original);

        let (url_encoded, _) = url_encode_transform(original).expect("url encode");
        let (url_decoded, _) = url_decode_transform(&url_encoded).expect("url decode");
        assert_eq!(url_decoded, original);

        let (base64_encoded, _) = base64_encode_transform(original).expect("base64 encode");
        let (base64_decoded, _) = base64_decode_transform(&base64_encoded).expect("base64 decode");
        assert_eq!(base64_decoded, original);
    }

    #[test]
    fn syntax_highlight_entrypoint_returns_ranges_for_code() {
        let highlights = syntax_highlights(
            "fn main() {\n    let answer = 42;\n}\n",
            LanguageTag::Rust,
            true,
        );
        assert!(
            !highlights.is_empty(),
            "expected syntect to produce at least one highlight span"
        );
    }

    #[test]
    fn menu_command_mapping_handles_core_tags() {
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_SHOW),
            Some(MenuCommand::ShowLauncher)
        ));
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_QUIT),
            Some(MenuCommand::QuitApp)
        ));
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_SYNTAX_ON),
            Some(MenuCommand::SetSyntaxHighlighting(true))
        ));
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_SYNTAX_OFF),
            Some(MenuCommand::SetSyntaxHighlighting(false))
        ));
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_SECRET_CLEAR_ON),
            Some(MenuCommand::SetSecretAutoClear(true))
        ));
        assert!(matches!(
            menu_command_from_tag(MENU_TAG_SECRET_CLEAR_OFF),
            Some(MenuCommand::SetSecretAutoClear(false))
        ));
    }
}

pub(crate) fn spawn_neural_init(storage: Arc<ClipboardStorage>) {
    if let Ok(mut status) = NEURAL_STATUS.lock() {
        *status = NeuralStatus::Loading;
    }
    std::thread::Builder::new()
        .name("pasta-neural-init".into())
        .spawn(move || {
            eprintln!("info: initializing neural embedder (may download model on first run)...");
            match neural_embed::NeuralEmbedder::try_new() {
                Ok(embedder) => {
                    storage.set_neural_embedder(Arc::new(embedder));
                    if let Ok(mut status) = NEURAL_STATUS.lock() {
                        *status = NeuralStatus::Ready;
                    }
                    eprintln!("info: neural embedder ready");
                }
                Err(err) => {
                    if let Ok(mut status) = NEURAL_STATUS.lock() {
                        *status = NeuralStatus::Failed;
                    }
                    eprintln!("warning: neural embedder unavailable: {err}");
                    eprintln!("warning: semantic search will use feature-hash only");
                }
            }
        })
        .ok();
}

#[cfg(target_os = "macos")]
fn main() {
    Application::new().run(|cx: &mut App| {
        ensure_launch_agent_registered();

        let (menu_tx, menu_rx) = mpsc::channel::<MenuCommand>();
        let _ = MENU_COMMAND_TX.set(menu_tx);

        let storage = match ClipboardStorage::bootstrap("PastaClipboard") {
            Ok(storage) => Arc::new(storage),
            Err(primary_err) => {
                eprintln!(
                    "warning: failed to initialize persistent clipboard storage: {primary_err}"
                );
                match ClipboardStorage::bootstrap_fallback("PastaClipboard") {
                    Ok(storage) => {
                        eprintln!("warning: using fallback clipboard storage for this session");
                        show_macos_notification(
                            "Pasta",
                            "Storage fallback mode is active for this session.",
                        );
                        Arc::new(storage)
                    }
                    Err(fallback_err) => {
                        eprintln!(
                            "error: failed to initialize clipboard storage fallback: {fallback_err}"
                        );
                        cx.quit();
                        return;
                    }
                }
            }
        };
        if let Some(initial_snapshot) = read_clipboard_snapshot()
            && !initial_snapshot.is_transient
        {
            let _ = if initial_snapshot.is_concealed {
                storage.upsert_clipboard_item_with_hint(&initial_snapshot.text, true)
            } else {
                storage.upsert_clipboard_item(&initial_snapshot.text)
            };
        }

        cx.set_global(StorageState {
            storage: storage.clone(),
        });

        spawn_neural_init(storage.clone());

        load_embedded_ui_font(cx);
        cx.bind_keys([
            KeyBinding::new("backspace", QueryBackspace, Some("PastaTextInput")),
            KeyBinding::new("left", QueryLeft, Some("PastaTextInput")),
            KeyBinding::new("right", QueryRight, Some("PastaTextInput")),
            KeyBinding::new("shift-left", QuerySelectLeft, Some("PastaTextInput")),
            KeyBinding::new("shift-right", QuerySelectRight, Some("PastaTextInput")),
            KeyBinding::new("cmd-a", QuerySelectAll, Some("PastaTextInput")),
            KeyBinding::new("cmd-v", QueryPaste, Some("PastaTextInput")),
            KeyBinding::new("cmd-c", QueryCopy, Some("PastaTextInput")),
            KeyBinding::new("cmd-x", QueryCut, Some("PastaTextInput")),
            KeyBinding::new("home", QueryHome, Some("PastaTextInput")),
            KeyBinding::new("end", QueryEnd, Some("PastaTextInput")),
            KeyBinding::new(
                "ctrl-cmd-space",
                QueryShowCharacterPalette,
                Some("PastaTextInput"),
            ),
        ]);

        let window = create_launcher_window(cx);
        cx.set_global(LauncherState { window });
        cx.set_global(AutoClearState::default());
        cx.set_global(SelfClipboardWriteState::default());
        configure_background_mode();
        setup_status_item(cx);
        setup_hotkey(cx);
        spawn_hotkey_listener(cx);
        spawn_menu_command_listener(cx, menu_rx);
        spawn_launcher_transition_loop(cx);
        spawn_clipboard_watcher(cx);

        cx.hide();
    });
}

#[cfg(target_os = "linux")]
fn main() {
    Application::new().run(|cx: &mut App| {
        let (menu_tx, menu_rx) = mpsc::channel::<MenuCommand>();
        let _ = MENU_COMMAND_TX.set(menu_tx);

        let storage = match ClipboardStorage::bootstrap("PastaClipboard") {
            Ok(storage) => Arc::new(storage),
            Err(primary_err) => {
                eprintln!(
                    "warning: failed to initialize persistent clipboard storage: {primary_err}"
                );
                match ClipboardStorage::bootstrap_fallback("PastaClipboard") {
                    Ok(storage) => {
                        eprintln!("warning: using fallback clipboard storage for this session");
                        show_macos_notification(
                            "Pasta",
                            "Storage fallback mode is active for this session.",
                        );
                        Arc::new(storage)
                    }
                    Err(fallback_err) => {
                        eprintln!(
                            "error: failed to initialize clipboard storage fallback: {fallback_err}"
                        );
                        cx.quit();
                        return;
                    }
                }
            }
        };

        if let Some(initial_snapshot) = read_clipboard_snapshot()
            && !initial_snapshot.is_transient
        {
            let _ = if initial_snapshot.is_concealed {
                storage.upsert_clipboard_item_with_hint(&initial_snapshot.text, true)
            } else {
                storage.upsert_clipboard_item(&initial_snapshot.text)
            };
        }

        cx.set_global(StorageState {
            storage: storage.clone(),
        });

        spawn_neural_init(storage.clone());

        load_embedded_ui_font(cx);
        cx.bind_keys([
            KeyBinding::new("backspace", QueryBackspace, Some("PastaTextInput")),
            KeyBinding::new("left", QueryLeft, Some("PastaTextInput")),
            KeyBinding::new("right", QueryRight, Some("PastaTextInput")),
            KeyBinding::new("shift-left", QuerySelectLeft, Some("PastaTextInput")),
            KeyBinding::new("shift-right", QuerySelectRight, Some("PastaTextInput")),
            KeyBinding::new("ctrl-a", QuerySelectAll, Some("PastaTextInput")),
            KeyBinding::new("ctrl-v", QueryPaste, Some("PastaTextInput")),
            KeyBinding::new("ctrl-c", QueryCopy, Some("PastaTextInput")),
            KeyBinding::new("ctrl-x", QueryCut, Some("PastaTextInput")),
            KeyBinding::new("home", QueryHome, Some("PastaTextInput")),
            KeyBinding::new("end", QueryEnd, Some("PastaTextInput")),
        ]);

        // On macOS, load_embedded_ui_font sets UiStyleState. The Linux stub
        // doesn't yet, so set a default here.
        cx.set_global(UiStyleState {
            family: "Meslo LG".into(),
            surface_alpha: 0.90,
            syntax_highlighting: true,
            secret_auto_clear: true,
            pasta_brain_enabled: true,
        });

        cx.set_global(LauncherState { window: None });
        cx.set_global(AutoClearState::default());
        cx.set_global(SelfClipboardWriteState::default());

        // Stubs — these are no-ops until Phases 2-4
        configure_background_mode();
        setup_status_item(cx);
        setup_hotkey(cx);

        // These runtime spawners need real implementations in later phases.
        // For now they just set up the event loops with stubbed platform calls.
        spawn_hotkey_listener(cx);
        spawn_menu_command_listener(cx, menu_rx);
        spawn_launcher_transition_loop(cx);
        spawn_clipboard_watcher(cx);
        show_launcher(cx);
    });
}
