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
#[cfg(target_os = "macos")]
const TOP_OFFSET: f32 = 146.0;
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.pasta.launcher";
const PREVIEW_LINE_LIMIT: usize = 4;
const PREVIEW_WRAP_RUN: usize = 96;
const WINDOW_OPEN_DURATION_MS: u64 = 120;
#[cfg(not(target_os = "linux"))]
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
    pub theme_mode: ThemeMode,
    pub syntax_highlighting: bool,
    pub secret_auto_clear: bool,
    pub pasta_brain_enabled: bool,
}

impl Global for UiStyleState {}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PersistedUiStyleState {
    pub(crate) family: String,
    pub(crate) surface_alpha: f32,
    #[serde(default)]
    pub(crate) theme_mode: ThemeMode,
    pub(crate) syntax_highlighting: bool,
    pub(crate) secret_auto_clear: bool,
    #[serde(default = "default_pasta_brain_enabled")]
    pub(crate) pasta_brain_enabled: bool,
}

fn default_pasta_brain_enabled() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub(crate) fn apply(self, appearance: WindowAppearance) -> WindowAppearance {
        match self {
            Self::System => appearance,
            Self::Light => WindowAppearance::Light,
            Self::Dark => WindowAppearance::Dark,
        }
    }
}

// Menu-item tag constants. Used by the macOS NSMenu dispatch table and by the
// Linux tray-menu unit tests; the Linux runtime dispatches without integer
// tags. Gate accordingly so release builds on Linux don't flag them unused.
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_SHOW: isize = 1;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_QUIT: isize = 2;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_FONT_BASE: isize = 100;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_ABOUT: isize = 200;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_SYNTAX_ON: isize = 300;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_SYNTAX_OFF: isize = 301;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_SECRET_CLEAR_ON: isize = 302;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_SECRET_CLEAR_OFF: isize = 303;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_BRAIN_ON: isize = 304;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_BRAIN_OFF: isize = 305;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_BRAIN_DOWNLOAD: isize = 306;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_THEME_SYSTEM: isize = 307;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_THEME_LIGHT: isize = 308;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_THEME_DARK: isize = 309;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_CLEAR_HISTORY: isize = 310;
#[cfg(any(target_os = "macos", test))]
pub(crate) const MENU_TAG_LAUNCH_AT_LOGIN: isize = 311;

static MENU_COMMAND_TX: OnceLock<mpsc::Sender<MenuCommand>> = OnceLock::new();

#[derive(Clone, Copy)]
pub(crate) enum MenuCommand {
    ShowLauncher,
    QuitApp,
    SetFont(FontChoice),
    ShowAbout,
    SetThemeMode(ThemeMode),
    SetSyntaxHighlighting(bool),
    SetSecretAutoClear(bool),
    SetPastaBrain(bool),
    DownloadBrain,
    RequestClearHistory,
    PerformClearHistory,
    ToggleLaunchAtLogin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NeuralStatus {
    Loading,
    Ready,
    Failed,
}

pub(crate) static NEURAL_STATUS: std::sync::Mutex<NeuralStatus> =
    std::sync::Mutex::new(NeuralStatus::Loading);

#[derive(Clone, Copy, Eq, PartialEq)]
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

    // Pin the fastembed cache to a stable, user-scoped location so the model isn't
    // re-downloaded whenever the app launches from a different working directory
    // (e.g. Finder vs. Terminal vs. the .app bundle). FASTEMBED_CACHE_DIR is read
    // by fastembed at TextEmbedding::try_new time.
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("pasta-launcher")
        .join("fastembed");
    let _ = fs::create_dir_all(&cache_dir);
    // SAFETY: called once during startup before any fastembed init reads the env;
    // no other thread is racing this write. (Rust 2024 marks set_var as unsafe.)
    unsafe {
        env::set_var("FASTEMBED_CACHE_DIR", &cache_dir);
    }
    let model_dir = cache_dir.join("models--Qdrant--all-MiniLM-L6-v2-onnx");
    let model_cached = model_dir.exists();

    std::thread::Builder::new()
        .name("pasta-neural-init".into())
        .spawn(move || {
            if !model_cached {
                show_macos_notification(
                    "Pasta Brain",
                    "Downloading semantic search model (~90 MB)…",
                );
            }
            eprintln!("info: initializing neural embedder (may download model on first run)...");
            match neural_embed::NeuralEmbedder::try_new() {
                Ok(embedder) => {
                    storage.set_neural_embedder(Arc::new(embedder));
                    if let Ok(mut status) = NEURAL_STATUS.lock() {
                        *status = NeuralStatus::Ready;
                    }
                    eprintln!("info: neural embedder ready");
                    if !model_cached {
                        show_macos_notification(
                            "Pasta Brain",
                            "Model downloaded. Semantic search is ready.",
                        );
                    }
                }
                Err(err) => {
                    if let Ok(mut status) = NEURAL_STATUS.lock() {
                        *status = NeuralStatus::Failed;
                    }
                    eprintln!("warning: neural embedder unavailable: {err}");
                    eprintln!("warning: semantic search will use feature-hash only");
                    show_macos_notification(
                        "Pasta Brain",
                        "Model unavailable — using keyword search. Retry from the menu bar.",
                    );
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

        // load_embedded_ui_font now loads fonts into the text system and
        // sets UiStyleState from persisted preferences (or defaults).

        cx.set_global(LauncherState { window: None });
        let background_anchor_window = create_background_anchor_window(cx);
        cx.set_global(BackgroundAnchorState {
            window: background_anchor_window,
        });
        cx.set_global(AutoClearState::default());
        cx.set_global(SelfClipboardWriteState::default());

        // Platform setup
        configure_background_mode();
        ensure_launch_agent_registered();
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
