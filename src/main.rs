#![allow(unexpected_cfgs)]

mod storage;

#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
use gpui::{
    App, Application, Bounds, ClickEvent, ClipboardItem, Context, CursorStyle,
    Element as GpuiElement, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, FontWeight, Global, GlobalElementId, InspectorElementId, KeyBinding,
    KeystrokeEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, Render, ScrollStrategy, ShapedLine, SharedString, Style, TextRun,
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
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use storage::{
    ClipboardItemType, ClipboardParameter, ClipboardRecord, ClipboardStorage,
    render_parameterized_content,
};

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
const LAUNCHER_WIDTH: f32 = 860.0;
#[cfg(target_os = "macos")]
const LAUNCHER_HEIGHT: f32 = 560.0;
#[cfg(target_os = "macos")]
const LAUNCHER_EXPANDED_HEIGHT: f32 = 760.0;
#[cfg(target_os = "macos")]
const TOP_OFFSET: f32 = 146.0;
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.pasta.launcher";
#[cfg(target_os = "macos")]
const PREVIEW_LINE_LIMIT: usize = 4;
#[cfg(target_os = "macos")]
const PREVIEW_WRAP_RUN: usize = 96;
#[cfg(target_os = "macos")]
const WINDOW_OPEN_DURATION_MS: u64 = 120;
#[cfg(target_os = "macos")]
const WINDOW_CLOSE_DURATION_MS: u64 = 95;
#[cfg(target_os = "macos")]
const WINDOW_CLOSE_EARLY_EXIT_ALPHA: f32 = 0.08;
#[cfg(target_os = "macos")]
const WINDOW_HEIGHT_ANIMATION_DURATION_MS: u64 = 90;
#[cfg(target_os = "macos")]
const WINDOW_HEIGHT_ANIMATION_SNAP: f32 = 0.5;
#[cfg(target_os = "macos")]
const WINDOW_HEIGHT_RESIZE_STEP: f32 = 1.0;
#[cfg(target_os = "macos")]
const MAX_VISIBLE_TAG_CHIPS: usize = 4;
#[cfg(target_os = "macos")]
const RESULTS_HEIGHT_NORMAL: f32 = 446.0;
#[cfg(target_os = "macos")]
const RESULT_ROW_HEIGHT: f32 = 110.0;
#[cfg(target_os = "macos")]
const NS_WINDOW_COLLECTION_BEHAVIOR_MOVE_TO_ACTIVE_SPACE: usize = 1 << 1;

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct StorageState {
    storage: Arc<ClipboardStorage>,
}

#[cfg(target_os = "macos")]
impl Global for StorageState {}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct UiStyleState {
    family: SharedString,
    surface_alpha: f32,
    syntax_highlighting: bool,
    secret_auto_clear: bool,
}

#[cfg(target_os = "macos")]
impl Global for UiStyleState {}

#[cfg(target_os = "macos")]
#[derive(Debug, Serialize, Deserialize)]
struct PersistedUiStyleState {
    family: String,
    surface_alpha: f32,
    syntax_highlighting: bool,
    secret_auto_clear: bool,
}

#[cfg(target_os = "macos")]
const MENU_TAG_SHOW: isize = 1;
#[cfg(target_os = "macos")]
const MENU_TAG_QUIT: isize = 2;
#[cfg(target_os = "macos")]
const MENU_TAG_FONT_BASE: isize = 100;
#[cfg(target_os = "macos")]
const MENU_TAG_ALPHA_BASE: isize = 200;
#[cfg(target_os = "macos")]
const MENU_TAG_SYNTAX_ON: isize = 300;
#[cfg(target_os = "macos")]
const MENU_TAG_SYNTAX_OFF: isize = 301;
#[cfg(target_os = "macos")]
const MENU_TAG_SECRET_CLEAR_ON: isize = 302;
#[cfg(target_os = "macos")]
const MENU_TAG_SECRET_CLEAR_OFF: isize = 303;

#[cfg(target_os = "macos")]
const TRANSPARENCY_LEVELS: [f32; 6] = [0.50, 0.60, 0.70, 0.80, 0.90, 1.00];

#[cfg(target_os = "macos")]
static MENU_COMMAND_TX: OnceLock<mpsc::Sender<MenuCommand>> = OnceLock::new();

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum MenuCommand {
    ShowLauncher,
    QuitApp,
    SetFont(FontChoice),
    SetTransparency(f32),
    SetSyntaxHighlighting(bool),
    SetSecretAutoClear(bool),
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum FontChoice {
    MesloLg,
    Iosevka,
    IbmPlexMono,
    JetBrainsMono,
    SourceCodePro,
    Monaspace,
    InputMono,
}

#[cfg(target_os = "macos")]
impl FontChoice {
    const ALL: [Self; 7] = [
        Self::MesloLg,
        Self::Iosevka,
        Self::IbmPlexMono,
        Self::JetBrainsMono,
        Self::SourceCodePro,
        Self::Monaspace,
        Self::InputMono,
    ];

    fn label(self) -> &'static str {
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

    fn candidates(self) -> &'static [&'static str] {
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

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum LauncherExitIntent {
    Hide,
    Quit,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TagEditorMode {
    Add,
    Remove,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParameterEditorStage {
    SelectValue,
    EnterName,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransformAction {
    ShellQuote,
    JsonEncode,
    JsonDecode,
    UrlEncode,
    UrlDecode,
    Base64Encode,
    Base64Decode,
    PublicCertPemInfo,
}

#[cfg(target_os = "macos")]
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
        _ => None,
    }
}

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "macos")]
mod transforms;
#[cfg(target_os = "macos")]
mod ui;

#[cfg(target_os = "macos")]
use app::*;
#[cfg(target_os = "macos")]
use platform::macos::*;
#[cfg(target_os = "macos")]
use transforms::*;
#[cfg(target_os = "macos")]
use ui::*;

#[cfg(all(target_os = "macos", test))]
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
        load_embedded_ui_font(cx);
        cx.bind_keys([
            KeyBinding::new("backspace", QueryBackspace, Some("PastaQueryInput")),
            KeyBinding::new("left", QueryLeft, Some("PastaQueryInput")),
            KeyBinding::new("right", QueryRight, Some("PastaQueryInput")),
            KeyBinding::new("shift-left", QuerySelectLeft, Some("PastaQueryInput")),
            KeyBinding::new("shift-right", QuerySelectRight, Some("PastaQueryInput")),
            KeyBinding::new("cmd-a", QuerySelectAll, Some("PastaQueryInput")),
            KeyBinding::new("cmd-v", QueryPaste, Some("PastaQueryInput")),
            KeyBinding::new("cmd-c", QueryCopy, Some("PastaQueryInput")),
            KeyBinding::new("cmd-x", QueryCut, Some("PastaQueryInput")),
            KeyBinding::new("home", QueryHome, Some("PastaQueryInput")),
            KeyBinding::new("end", QueryEnd, Some("PastaQueryInput")),
            KeyBinding::new(
                "ctrl-cmd-space",
                QueryShowCharacterPalette,
                Some("PastaQueryInput"),
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

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This app currently supports macOS only.");
}
