mod storage;

#[cfg(target_os = "macos")]
use std::{
    borrow::Cow,
    env,
    ffi::CStr,
    fs,
    ops::Range,
    sync::{Arc, OnceLock, mpsc},
    time::Duration,
};

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
    App, Application, Bounds, ClipboardItem, Context, FontWeight, Global, HighlightStyle,
    KeystrokeEvent, Render, ScrollHandle, SharedString, StyledText, Window, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, rgb, rgba, size,
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
use storage::{ClipboardItemType, ClipboardRecord, ClipboardStorage};

#[cfg(target_os = "macos")]
const LAUNCHER_WIDTH: f32 = 860.0;
#[cfg(target_os = "macos")]
const LAUNCHER_HEIGHT: f32 = 430.0;
#[cfg(target_os = "macos")]
const TOP_OFFSET: f32 = 146.0;
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "com.pasta.launcher";
#[cfg(target_os = "macos")]
const PREVIEW_LINE_LIMIT: usize = 4;
#[cfg(target_os = "macos")]
const PREVIEW_WRAP_RUN: usize = 96;
#[cfg(target_os = "macos")]
const WINDOW_TRANSITION_SPEED: f32 = 0.44;
#[cfg(target_os = "macos")]
const WINDOW_TRANSITION_SNAP: f32 = 0.06;
#[cfg(target_os = "macos")]
const WINDOW_TRANSITION_CLOSE_CUTOFF: f32 = 0.12;

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
}

#[cfg(target_os = "macos")]
impl Global for UiStyleState {}

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
            // Input Mono is commercial; this choice works only when user has it installed.
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
struct LauncherView {
    storage: Arc<ClipboardStorage>,
    font_family: SharedString,
    surface_alpha: f32,
    syntax_highlighting: bool,
    results_scroll: ScrollHandle,
    query: String,
    items: Vec<ClipboardRecord>,
    selected_index: usize,
    transition_alpha: f32,
    transition_target: f32,
    pending_exit: Option<LauncherExitIntent>,
}

#[cfg(target_os = "macos")]
impl LauncherView {
    fn new(
        storage: Arc<ClipboardStorage>,
        font_family: SharedString,
        surface_alpha: f32,
        syntax_highlighting: bool,
    ) -> Self {
        let mut view = Self {
            storage,
            font_family,
            surface_alpha,
            syntax_highlighting,
            results_scroll: ScrollHandle::new(),
            query: String::new(),
            items: Vec::new(),
            selected_index: 0,
            transition_alpha: 1.0,
            transition_target: 1.0,
            pending_exit: None,
        };
        view.refresh_items();
        view
    }

    fn reset_for_show(&mut self) {
        self.query.clear();
        self.selected_index = 0;
        self.refresh_items();
        if !self.items.is_empty() {
            self.results_scroll.scroll_to_top_of_item(0);
        }
    }

    fn begin_open_transition(&mut self) {
        self.pending_exit = None;
        self.transition_alpha = 0.0;
        self.transition_target = 1.0;
    }

    fn begin_close_transition(&mut self, intent: LauncherExitIntent) {
        self.pending_exit = Some(intent);
        self.transition_target = 0.0;
    }

    fn transition_running(&self) -> bool {
        (self.transition_alpha - self.transition_target).abs() > 0.001
            || (self.transition_target == 0.0 && self.pending_exit.is_some())
    }

    fn tick_transition(&mut self) -> Option<LauncherExitIntent> {
        let delta = self.transition_target - self.transition_alpha;
        if delta.abs() <= WINDOW_TRANSITION_SNAP {
            self.transition_alpha = self.transition_target;
        } else {
            self.transition_alpha =
                (self.transition_alpha + delta * WINDOW_TRANSITION_SPEED).clamp(0.0, 1.0);
        }

        if self.transition_target <= 0.0
            && self.transition_alpha <= WINDOW_TRANSITION_CLOSE_CUTOFF
            && self.pending_exit.is_some()
        {
            self.transition_alpha = 0.0;
            return self.pending_exit.take();
        }

        None
    }

    fn refresh_items(&mut self) {
        self.items = self
            .storage
            .search_items(&self.query, 48)
            .unwrap_or_else(|_| Vec::new());
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }

    fn move_selection(&mut self, direction: i32, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }

        if direction > 0 {
            if self.selected_index + 1 < self.items.len() {
                self.selected_index += 1;
            }
        } else if direction < 0 {
            self.selected_index = self.selected_index.saturating_sub(1);
        }

        self.results_scroll.scroll_to_item(self.selected_index);
        cx.notify();
    }

    fn copy_selected_to_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.items.get(self.selected_index) {
            cx.write_to_clipboard(ClipboardItem::new_string(item.content.clone()));
            self.begin_close_transition(LauncherExitIntent::Hide);
            cx.notify();
        }
    }

    fn copy_index_to_clipboard(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_index = index;
        self.results_scroll.scroll_to_item(self.selected_index);
        self.copy_selected_to_clipboard(cx);
    }

    fn delete_selected_item(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };

        match self.storage.delete_item(item_id) {
            Ok(_) => {
                self.refresh_items();
                if !self.items.is_empty() {
                    self.results_scroll.scroll_to_item(self.selected_index);
                }
                cx.notify();
            }
            Err(err) => {
                eprintln!("warning: failed to delete clipboard item: {err}");
            }
        }
    }

    fn update_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        self.selected_index = 0;
        self.refresh_items();
        if !self.items.is_empty() {
            self.results_scroll.scroll_to_top_of_item(0);
        }
        cx.notify();
    }

    fn handle_keystroke(&mut self, event: &KeystrokeEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();

        if key == "escape" || key == "esc" {
            self.begin_close_transition(LauncherExitIntent::Hide);
            cx.notify();
            return;
        }

        match key {
            "up" | "arrowup" => {
                self.move_selection(-1, cx);
                return;
            }
            "down" | "arrowdown" => {
                self.move_selection(1, cx);
                return;
            }
            "enter" | "return" => {
                self.copy_selected_to_clipboard(cx);
                return;
            }
            "delete" | "forwarddelete" => {
                self.delete_selected_item(cx);
                return;
            }
            "d" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.delete_selected_item(cx);
                return;
            }
            "q" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.begin_close_transition(LauncherExitIntent::Quit);
                cx.notify();
                return;
            }
            "backspace"
                if modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.function =>
            {
                self.delete_selected_item(cx);
                return;
            }
            "backspace" if no_modifiers => {
                let mut query = self.query.clone();
                query.pop();
                self.update_query(query, cx);
                return;
            }
            _ => {}
        }

        if let Some(character) = typed_character(event) {
            let mut query = self.query.clone();
            query.push(character);
            self.update_query(query, cx);
        }
    }
}

#[cfg(target_os = "macos")]
impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette_for(window.appearance(), self.surface_alpha);
        let query_display = if self.query.is_empty() {
            "Search snippets, commands, and passwords…".to_owned()
        } else {
            self.query.clone()
        };
        let query_color = if self.query.is_empty() {
            palette.query_placeholder
        } else {
            palette.query_active
        };

        let mut results = div()
            .id("results-list")
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .h(px(316.0))
            .track_scroll(&self.results_scroll)
            .overflow_y_scroll();

        if self.items.is_empty() {
            results = results.child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(palette.muted_text)
                    .text_sm()
                    .child("Clipboard is empty. Copy text/code/commands to get started."),
            );
        } else {
            for (ix, item) in self.items.iter().enumerate() {
                let is_selected = ix == self.selected_index;
                let item_kind = item.item_type.label().to_owned();
                let item_preview = preview_content(&item.content);
                let item_created = format_timestamp(&item.created_at);
                let detected_language = detect_language(item.item_type, &item.content);

                let mut item_meta = div().flex().items_center().gap_2().child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(type_color(item.item_type, palette.dark))
                        .child(item_kind),
                );
                if let Some(language) = detected_language {
                    item_meta = item_meta.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(language_color(language, palette.dark))
                            .child(language.label()),
                    );
                }

                let mut row = div()
                    .id(("result", item.id as u64))
                    .w_full()
                    .p_1()
                    .rounded_lg()
                    .bg(if is_selected {
                        palette.selected_bg
                    } else {
                        rgba(0x00000000)
                    })
                    .hover({
                        let row_hover = palette.row_hover_bg;
                        move |style| style.bg(row_hover)
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.copy_index_to_clipboard(ix, cx);
                    }))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .child(item_meta)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(palette.row_meta_text)
                                    .child(item_created),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_sm()
                            .text_color(palette.row_text)
                            .whitespace_normal()
                            .line_clamp(4)
                            .child(syntax_styled_text(
                                &item_preview,
                                detected_language,
                                self.syntax_highlighting,
                                palette.dark,
                            )),
                    );

                if is_selected {
                    row = row.border_1().border_color(palette.selected_border);
                }

                results = results.child(row);
            }
        }

        let mut panel = div()
            .size_full()
            .font_family(self.font_family.clone())
            .font_weight(FontWeight::LIGHT)
            .opacity(self.transition_alpha)
            .bg(palette.window_bg)
            .border_1()
            .border_color(palette.window_border)
            .rounded_2xl()
            .overflow_hidden();
        if self.transition_target > 0.0 && self.transition_alpha > 0.35 {
            panel = panel.shadow_xl();
        }

        panel
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.title_text)
                            .child("PASTA CLIPBOARD"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("OPTION+SPACE"),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_lg()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(query_color)
                    .child(query_display),
            )
            .child(div().w_full().h(px(1.0)).bg(palette.list_divider))
            .child(results)
            .child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(palette.muted_text)
                    .child(
                        "Type to search • Enter/click copy • ⌘D delete • Syntax menu: Enabled/Disabled • Esc close • ⌘Q quit",
                    ),
            )
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    window_bg: gpui::Rgba,
    window_border: gpui::Rgba,
    title_text: gpui::Rgba,
    query_placeholder: gpui::Rgba,
    query_active: gpui::Rgba,
    muted_text: gpui::Rgba,
    list_divider: gpui::Rgba,
    row_text: gpui::Rgba,
    row_meta_text: gpui::Rgba,
    row_hover_bg: gpui::Rgba,
    selected_bg: gpui::Rgba,
    selected_border: gpui::Rgba,
}

#[cfg(target_os = "macos")]
fn palette_for(appearance: WindowAppearance, surface_alpha: f32) -> Palette {
    let dark = matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );

    let mut palette = if dark {
        Palette {
            dark,
            window_bg: rgba(0x0b0f14b8),
            window_border: rgba(0xffffff16),
            title_text: rgba(0xcbd5e1d9),
            query_placeholder: rgba(0x94a3b8d9),
            query_active: rgba(0xf8fafcfa),
            muted_text: rgba(0x94a3b8d0),
            list_divider: rgba(0xffffff14),
            row_text: rgba(0xe2e8f0f5),
            row_meta_text: rgba(0x94a3b8cc),
            row_hover_bg: rgba(0xffffff0c),
            selected_bg: rgba(0xffffff10),
            selected_border: rgba(0xffffff22),
        }
    } else {
        Palette {
            dark,
            window_bg: rgba(0xf8fafcb8),
            window_border: rgba(0x0f172a26),
            title_text: rgba(0x334155dd),
            query_placeholder: rgba(0x64748bcc),
            query_active: rgba(0x0f172af2),
            muted_text: rgba(0x475569cc),
            list_divider: rgba(0x3341551a),
            row_text: rgba(0x0f172ae8),
            row_meta_text: rgba(0x475569b3),
            row_hover_bg: rgba(0x0f172a0c),
            selected_bg: rgba(0x0f172a08),
            selected_border: rgba(0x0f172a24),
        }
    };

    let alpha_scale = surface_alpha.clamp(0.45, 1.0);
    palette.window_bg = scale_alpha(palette.window_bg, alpha_scale);
    palette.window_border = scale_alpha(palette.window_border, alpha_scale);
    palette.list_divider = scale_alpha(palette.list_divider, alpha_scale);
    palette.row_hover_bg = scale_alpha(palette.row_hover_bg, alpha_scale);
    palette.selected_bg = scale_alpha(palette.selected_bg, alpha_scale);
    palette.selected_border = scale_alpha(palette.selected_border, alpha_scale);

    palette
}

#[cfg(target_os = "macos")]
fn scale_alpha(color: gpui::Rgba, scale: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: (color.a * scale).clamp(0.0, 1.0),
    }
}

#[cfg(target_os = "macos")]
fn typed_character(event: &KeystrokeEvent) -> Option<char> {
    let modifiers = &event.keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }

    if event.keystroke.key == "space" {
        return Some(' ');
    }

    let candidate = event
        .keystroke
        .key_char
        .as_deref()
        .unwrap_or(event.keystroke.key.as_str());
    let mut chars = candidate.chars();
    let first = chars.next()?;
    if chars.next().is_some() || first.is_control() {
        return None;
    }

    Some(first)
}

#[cfg(target_os = "macos")]
fn type_color(item_type: ClipboardItemType, dark: bool) -> gpui::Hsla {
    match item_type {
        ClipboardItemType::Text => {
            if dark {
                rgb(0x38bdf8).into()
            } else {
                rgb(0x0369a1).into()
            }
        }
        ClipboardItemType::Code => {
            if dark {
                rgb(0x34d399).into()
            } else {
                rgb(0x047857).into()
            }
        }
        ClipboardItemType::Command => {
            if dark {
                rgb(0xfbbf24).into()
            } else {
                rgb(0xb45309).into()
            }
        }
        ClipboardItemType::Password => {
            if dark {
                rgb(0xf472b6).into()
            } else {
                rgb(0x9d174d).into()
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn preview_content(content: &str) -> String {
    let normalized = content.replace('\r', "").replace('\t', "    ");
    let wrapped = wrap_long_words(&normalized, PREVIEW_WRAP_RUN);
    let mut lines = wrapped.lines();
    let preview: Vec<&str> = lines.by_ref().take(PREVIEW_LINE_LIMIT).collect();
    let has_more = lines.next().is_some();

    if has_more {
        let mut joined = preview.join("\n");
        joined.push('…');
        joined
    } else {
        preview.join("\n")
    }
}

#[cfg(target_os = "macos")]
fn wrap_long_words(input: &str, max_run: usize) -> String {
    let mut out = String::with_capacity(input.len() + (input.len() / max_run.max(1)));
    let mut run = 0_usize;

    for ch in input.chars() {
        out.push(ch);
        if ch == '\n' || ch.is_whitespace() {
            run = 0;
            continue;
        }

        run += 1;
        if run >= max_run {
            out.push('\n');
            run = 0;
        }
    }

    out
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum LanguageTag {
    Bash,
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    Cpp,
    Sql,
    Json,
    Yaml,
    Html,
    Css,
    Markdown,
    Toml,
    Code,
}

#[cfg(target_os = "macos")]
impl LanguageTag {
    fn label(&self) -> &'static str {
        match self {
            Self::Bash => "BASH",
            Self::Rust => "RUST",
            Self::Python => "PY",
            Self::TypeScript => "TS",
            Self::JavaScript => "JS",
            Self::Go => "GO",
            Self::Java => "JAVA",
            Self::Cpp => "C++",
            Self::Sql => "SQL",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Markdown => "MD",
            Self::Toml => "TOML",
            Self::Code => "CODE",
        }
    }
}

#[cfg(target_os = "macos")]
fn detect_language(item_type: ClipboardItemType, content: &str) -> Option<LanguageTag> {
    if item_type == ClipboardItemType::Password {
        return None;
    }

    if item_type == ClipboardItemType::Command {
        return Some(LanguageTag::Bash);
    }

    let text = content.trim();
    if text.is_empty() {
        return None;
    }

    let lower = text.to_ascii_lowercase();

    if lower.contains("[package]") || lower.contains("cargo.toml") {
        return Some(LanguageTag::Toml);
    }
    if lower.contains("```")
        || lower
            .lines()
            .any(|line| line.trim_start().starts_with("# "))
    {
        return Some(LanguageTag::Markdown);
    }
    if looks_like_json(text, &lower) {
        return Some(LanguageTag::Json);
    }
    if looks_like_yaml(text) {
        return Some(LanguageTag::Yaml);
    }
    if lower.contains("<html") || lower.contains("</") || lower.contains("<div") {
        return Some(LanguageTag::Html);
    }
    if lower.contains('{')
        && lower.contains('}')
        && (lower.contains(':') || lower.contains(";"))
        && (lower.contains("color:") || lower.contains("display:") || lower.contains("margin:"))
    {
        return Some(LanguageTag::Css);
    }
    if contains_any(
        &lower,
        &[
            "select ",
            "insert into ",
            "update ",
            "delete from ",
            "where ",
        ],
    ) && lower.contains(" from ")
    {
        return Some(LanguageTag::Sql);
    }
    if contains_any(&lower, &["fn ", "impl ", "mut ", "let ", "::", "cargo "]) {
        return Some(LanguageTag::Rust);
    }
    if contains_any(
        &lower,
        &[
            "interface ",
            "type ",
            ": string",
            ": number",
            " as const",
            "readonly ",
            "import type ",
        ],
    ) {
        return Some(LanguageTag::TypeScript);
    }
    if contains_any(
        &lower,
        &[
            "function ",
            "console.log",
            "=>",
            "module.exports",
            "require(",
        ],
    ) {
        return Some(LanguageTag::JavaScript);
    }
    if contains_any(
        &lower,
        &["def ", "import ", "from ", "print(", "__name__", "lambda "],
    ) && text.contains(':')
    {
        return Some(LanguageTag::Python);
    }
    if contains_any(&lower, &["package main", "func ", "fmt.", "go "]) {
        return Some(LanguageTag::Go);
    }
    if contains_any(
        &lower,
        &[
            "public class",
            "public static void main",
            "system.out.println",
        ],
    ) {
        return Some(LanguageTag::Java);
    }
    if contains_any(&lower, &["#include", "std::", "int main(", "cout <<"]) {
        return Some(LanguageTag::Cpp);
    }

    if item_type == ClipboardItemType::Code {
        return Some(LanguageTag::Code);
    }

    None
}

#[cfg(target_os = "macos")]
fn language_color(language: LanguageTag, dark: bool) -> gpui::Hsla {
    let color = match language {
        LanguageTag::Bash => {
            if dark {
                rgb(0x84cc16)
            } else {
                rgb(0x4d7c0f)
            }
        }
        LanguageTag::Rust => {
            if dark {
                rgb(0xfb923c)
            } else {
                rgb(0xc2410c)
            }
        }
        LanguageTag::Python => {
            if dark {
                rgb(0xfacc15)
            } else {
                rgb(0xa16207)
            }
        }
        LanguageTag::TypeScript => {
            if dark {
                rgb(0x60a5fa)
            } else {
                rgb(0x1d4ed8)
            }
        }
        LanguageTag::JavaScript => {
            if dark {
                rgb(0xfacc15)
            } else {
                rgb(0xa16207)
            }
        }
        LanguageTag::Go => {
            if dark {
                rgb(0x67e8f9)
            } else {
                rgb(0x0e7490)
            }
        }
        LanguageTag::Java => {
            if dark {
                rgb(0xfda4af)
            } else {
                rgb(0xbe123c)
            }
        }
        LanguageTag::Cpp => {
            if dark {
                rgb(0xa78bfa)
            } else {
                rgb(0x6d28d9)
            }
        }
        LanguageTag::Sql => {
            if dark {
                rgb(0x5eead4)
            } else {
                rgb(0x0f766e)
            }
        }
        LanguageTag::Json => {
            if dark {
                rgb(0xfbbf24)
            } else {
                rgb(0xb45309)
            }
        }
        LanguageTag::Yaml => {
            if dark {
                rgb(0xf9a8d4)
            } else {
                rgb(0xbe185d)
            }
        }
        LanguageTag::Html => {
            if dark {
                rgb(0xfdba74)
            } else {
                rgb(0xc2410c)
            }
        }
        LanguageTag::Css => {
            if dark {
                rgb(0x93c5fd)
            } else {
                rgb(0x1d4ed8)
            }
        }
        LanguageTag::Markdown => {
            if dark {
                rgb(0xc4b5fd)
            } else {
                rgb(0x7c3aed)
            }
        }
        LanguageTag::Toml => {
            if dark {
                rgb(0xfca5a5)
            } else {
                rgb(0xb91c1c)
            }
        }
        LanguageTag::Code => {
            if dark {
                rgb(0x34d399)
            } else {
                rgb(0x047857)
            }
        }
    };

    color.into()
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SyntaxClass {
    Comment,
    String,
    Keyword,
    Number,
    Command,
    Flag,
}

#[cfg(target_os = "macos")]
fn syntax_styled_text(
    text: &str,
    language: Option<LanguageTag>,
    syntax_enabled: bool,
    dark: bool,
) -> StyledText {
    let styled = StyledText::new(text.to_owned());
    if !syntax_enabled {
        return styled;
    }

    let Some(language) = language else {
        return styled;
    };

    let highlights = syntax_highlights(text, language, dark);
    if highlights.is_empty() {
        styled
    } else {
        styled.with_highlights(highlights)
    }
}

#[cfg(target_os = "macos")]
fn syntax_highlights(
    text: &str,
    language: LanguageTag,
    dark: bool,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut slots: Vec<Option<SyntaxClass>> = vec![None; text.len()];

    for range in collect_string_ranges(text) {
        assign_syntax_class(&mut slots, text, range, SyntaxClass::String);
    }
    for range in collect_comment_ranges(text, language) {
        assign_syntax_class(&mut slots, text, range, SyntaxClass::Comment);
    }
    for range in collect_keyword_ranges(text, language) {
        assign_syntax_class(&mut slots, text, range, SyntaxClass::Keyword);
    }
    for range in collect_number_ranges(text) {
        assign_syntax_class(&mut slots, text, range, SyntaxClass::Number);
    }

    if matches!(language, LanguageTag::Bash) {
        let (commands, flags) = collect_command_and_flag_ranges(text);
        for range in commands {
            assign_syntax_class(&mut slots, text, range, SyntaxClass::Command);
        }
        for range in flags {
            assign_syntax_class(&mut slots, text, range, SyntaxClass::Flag);
        }
    }

    let mut highlights = Vec::new();
    let mut ix = 0;
    while ix < slots.len() {
        let Some(class) = slots[ix] else {
            ix += 1;
            continue;
        };
        let start = ix;
        while ix < slots.len() && slots[ix] == Some(class) {
            ix += 1;
        }
        if text.is_char_boundary(start) && text.is_char_boundary(ix) && start < ix {
            highlights.push((start..ix, syntax_style(class, dark)));
        }
    }

    highlights
}

#[cfg(target_os = "macos")]
fn syntax_style(class: SyntaxClass, dark: bool) -> HighlightStyle {
    let color = match class {
        SyntaxClass::Comment => {
            if dark {
                rgb(0x86efac)
            } else {
                rgb(0x166534)
            }
        }
        SyntaxClass::String => {
            if dark {
                rgb(0xfca5a5)
            } else {
                rgb(0x9f1239)
            }
        }
        SyntaxClass::Keyword => {
            if dark {
                rgb(0x93c5fd)
            } else {
                rgb(0x1d4ed8)
            }
        }
        SyntaxClass::Number => {
            if dark {
                rgb(0xfcd34d)
            } else {
                rgb(0xa16207)
            }
        }
        SyntaxClass::Command => {
            if dark {
                rgb(0xc4b5fd)
            } else {
                rgb(0x6d28d9)
            }
        }
        SyntaxClass::Flag => {
            if dark {
                rgb(0x67e8f9)
            } else {
                rgb(0x0e7490)
            }
        }
    };

    let hsla: gpui::Hsla = color.into();
    HighlightStyle::color(hsla)
}

#[cfg(target_os = "macos")]
fn assign_syntax_class(
    slots: &mut [Option<SyntaxClass>],
    text: &str,
    range: Range<usize>,
    class: SyntaxClass,
) {
    if range.start >= range.end || range.end > text.len() {
        return;
    }
    if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
        return;
    }

    for slot in slots.iter_mut().take(range.end).skip(range.start) {
        if slot.is_none() {
            *slot = Some(class);
        }
    }
}

#[cfg(target_os = "macos")]
fn collect_string_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut ix = 0;

    while ix < bytes.len() {
        let byte = bytes[ix];
        if byte == b'"' || byte == b'\'' || byte == b'`' {
            let quote = byte;
            let start = ix;
            ix += 1;
            while ix < bytes.len() {
                if bytes[ix] == b'\\' {
                    ix += 1;
                    if ix < bytes.len() {
                        ix += utf8_char_width(bytes[ix]);
                    }
                    continue;
                }
                if bytes[ix] == quote {
                    ix += 1;
                    break;
                }
                ix += utf8_char_width(bytes[ix]);
            }
            ranges.push(start..ix.min(bytes.len()));
            continue;
        }
        ix += utf8_char_width(bytes[ix]);
    }

    ranges
}

#[cfg(target_os = "macos")]
fn collect_comment_ranges(text: &str, language: LanguageTag) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let line_prefixes: &[&str] = match language {
        LanguageTag::Bash | LanguageTag::Python | LanguageTag::Yaml | LanguageTag::Toml => &["#"],
        LanguageTag::Sql => &["--"],
        LanguageTag::Rust
        | LanguageTag::TypeScript
        | LanguageTag::JavaScript
        | LanguageTag::Go
        | LanguageTag::Java
        | LanguageTag::Cpp
        | LanguageTag::Css
        | LanguageTag::Code => &["//"],
        _ => &[],
    };

    let mut line_offset = 0;
    for line in text.split_inclusive('\n') {
        let line_body = line.strip_suffix('\n').unwrap_or(line);
        for prefix in line_prefixes {
            if let Some(pos) = line_body.find(prefix) {
                ranges.push((line_offset + pos)..(line_offset + line_body.len()));
                break;
            }
        }
        line_offset += line.len();
    }

    if matches!(
        language,
        LanguageTag::Rust
            | LanguageTag::TypeScript
            | LanguageTag::JavaScript
            | LanguageTag::Go
            | LanguageTag::Java
            | LanguageTag::Cpp
            | LanguageTag::Css
            | LanguageTag::Code
    ) {
        ranges.extend(collect_block_comment_ranges(text, "/*", "*/"));
    }
    if matches!(language, LanguageTag::Html | LanguageTag::Markdown) {
        ranges.extend(collect_block_comment_ranges(text, "<!--", "-->"));
    }

    ranges
}

#[cfg(target_os = "macos")]
fn collect_block_comment_ranges(
    text: &str,
    start_marker: &str,
    end_marker: &str,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(start_rel) = text[cursor..].find(start_marker) else {
            break;
        };
        let start = cursor + start_rel;
        let content_start = start + start_marker.len();
        if let Some(end_rel) = text[content_start..].find(end_marker) {
            let end = content_start + end_rel + end_marker.len();
            ranges.push(start..end);
            cursor = end;
        } else {
            ranges.push(start..text.len());
            break;
        }
    }
    ranges
}

#[cfg(target_os = "macos")]
fn collect_keyword_ranges(text: &str, language: LanguageTag) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for keyword in syntax_keywords(language) {
        for (start, _) in text.match_indices(keyword) {
            let end = start + keyword.len();
            if is_word_boundary(text, start, end) {
                ranges.push(start..end);
            }
        }
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges
}

#[cfg(target_os = "macos")]
fn syntax_keywords(language: LanguageTag) -> &'static [&'static str] {
    match language {
        LanguageTag::Rust => &[
            "fn", "let", "mut", "impl", "struct", "enum", "match", "if", "else", "use", "pub",
            "crate", "mod", "async", "await", "return", "trait",
        ],
        LanguageTag::TypeScript | LanguageTag::JavaScript => &[
            "const",
            "let",
            "var",
            "function",
            "return",
            "if",
            "else",
            "import",
            "from",
            "export",
            "class",
            "interface",
            "type",
            "async",
            "await",
            "new",
            "extends",
        ],
        LanguageTag::Python => &[
            "def", "class", "import", "from", "return", "if", "elif", "else", "for", "while",
            "with", "try", "except", "lambda", "yield",
        ],
        LanguageTag::Go => &[
            "package", "import", "func", "return", "if", "else", "for", "range", "struct", "type",
            "var", "const", "go", "defer",
        ],
        LanguageTag::Java | LanguageTag::Cpp => &[
            "class",
            "public",
            "private",
            "protected",
            "static",
            "void",
            "return",
            "if",
            "else",
            "for",
            "while",
            "new",
            "int",
            "String",
            "include",
        ],
        LanguageTag::Sql => &[
            "select", "from", "where", "insert", "into", "update", "delete", "join", "group",
            "order", "by", "limit", "having", "and", "or",
        ],
        LanguageTag::Json => &["true", "false", "null"],
        LanguageTag::Yaml | LanguageTag::Toml => &["true", "false"],
        LanguageTag::Bash => &[
            "if", "then", "else", "fi", "for", "in", "do", "done", "case", "esac", "function",
            "export", "local",
        ],
        LanguageTag::Html => &["html", "head", "body", "div", "span", "script", "style"],
        LanguageTag::Css => &[
            "display",
            "position",
            "color",
            "background",
            "font",
            "margin",
            "padding",
            "border",
        ],
        LanguageTag::Markdown => &["```", "#", "##", "###"],
        LanguageTag::Code => &["if", "else", "for", "while", "return", "class", "function"],
    }
}

#[cfg(target_os = "macos")]
fn collect_number_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut ix = 0;

    while ix < bytes.len() {
        if bytes[ix].is_ascii_digit() && (ix == 0 || !is_ident_byte(bytes[ix - 1])) {
            let start = ix;
            ix += 1;
            while ix < bytes.len()
                && (bytes[ix].is_ascii_digit() || bytes[ix] == b'.' || bytes[ix] == b'_')
            {
                ix += 1;
            }
            ranges.push(start..ix);
            continue;
        }
        ix += 1;
    }

    ranges
}

#[cfg(target_os = "macos")]
fn collect_command_and_flag_ranges(text: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut commands = Vec::new();
    let mut flags = Vec::new();

    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let bytes = body.as_bytes();
        let mut ix = 0;
        while ix < bytes.len() && bytes[ix].is_ascii_whitespace() {
            ix += 1;
        }
        if ix >= bytes.len() || bytes[ix] == b'#' {
            offset += line.len();
            continue;
        }
        if bytes[ix] == b'$' {
            ix += 1;
            while ix < bytes.len() && bytes[ix].is_ascii_whitespace() {
                ix += 1;
            }
        }

        let cmd_start = ix;
        while ix < bytes.len() && !bytes[ix].is_ascii_whitespace() {
            ix += 1;
        }
        if cmd_start < ix {
            commands.push((offset + cmd_start)..(offset + ix));
        }

        while ix < bytes.len() {
            while ix < bytes.len() && bytes[ix].is_ascii_whitespace() {
                ix += 1;
            }
            let token_start = ix;
            while ix < bytes.len() && !bytes[ix].is_ascii_whitespace() {
                ix += 1;
            }
            if token_start < ix && bytes[token_start] == b'-' {
                flags.push((offset + token_start)..(offset + ix));
            }
        }

        offset += line.len();
    }

    (commands, flags)
}

#[cfg(target_os = "macos")]
fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();
    let before_is_ident = start
        .checked_sub(1)
        .and_then(|ix| bytes.get(ix))
        .copied()
        .is_some_and(is_ident_byte);
    let after_is_ident = bytes.get(end).copied().is_some_and(is_ident_byte);

    !before_is_ident && !after_is_ident
}

#[cfg(target_os = "macos")]
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(target_os = "macos")]
fn utf8_char_width(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(target_os = "macos")]
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(target_os = "macos")]
fn looks_like_json(text: &str, lower: &str) -> bool {
    let trimmed = text.trim();
    let wrapped = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    wrapped && lower.contains(':') && trimmed.contains('"')
}

#[cfg(target_os = "macos")]
fn looks_like_yaml(text: &str) -> bool {
    let mut has_pairs = 0_usize;
    for line in text.lines().take(12) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.contains(':')
            && !trimmed.contains('{')
            && !trimmed.contains('}')
            && !trimmed.contains(';')
        {
            has_pairs += 1;
        }
    }

    has_pairs >= 2
}

#[cfg(target_os = "macos")]
fn format_timestamp(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "now".to_owned())
}

#[cfg(target_os = "macos")]
struct HotkeyRegistration {
    _manager: GlobalHotKeyManager,
    hotkey_id: u32,
}

#[cfg(target_os = "macos")]
impl Global for HotkeyRegistration {}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct LauncherState {
    window: Option<WindowHandle<LauncherView>>,
}

#[cfg(target_os = "macos")]
impl Global for LauncherState {}

#[cfg(target_os = "macos")]
struct StatusItemRegistration {
    _status_item: StrongPtr,
    _menu: StrongPtr,
    _handler: StrongPtr,
}

#[cfg(target_os = "macos")]
impl Global for StatusItemRegistration {}

#[cfg(target_os = "macos")]
fn configure_background_mode() {
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
    }
}

#[cfg(target_os = "macos")]
fn menu_action_handler_class() -> *const Class {
    static CLASS: OnceLock<usize> = OnceLock::new();
    *CLASS.get_or_init(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("PastaMenuActionHandler", superclass)
            .expect("failed to create PastaMenuActionHandler class");
        decl.add_method(
            sel!(menuAction:),
            menu_action as extern "C" fn(&Object, Sel, id),
        );
        (decl.register() as *const Class) as usize
    }) as *const Class
}

#[cfg(target_os = "macos")]
extern "C" fn menu_action(_this: &Object, _cmd: Sel, sender: id) {
    unsafe {
        let tag: isize = msg_send![sender, tag];
        let command = menu_command_from_tag(tag);
        if let (Some(command), Some(tx)) = (command, MENU_COMMAND_TX.get()) {
            let _ = tx.send(command);
        }
    }
}

#[cfg(target_os = "macos")]
fn menu_command_from_tag(tag: isize) -> Option<MenuCommand> {
    if tag == MENU_TAG_SHOW {
        return Some(MenuCommand::ShowLauncher);
    }
    if tag == MENU_TAG_QUIT {
        return Some(MenuCommand::QuitApp);
    }

    if (MENU_TAG_FONT_BASE..MENU_TAG_FONT_BASE + FontChoice::ALL.len() as isize).contains(&tag) {
        let index = (tag - MENU_TAG_FONT_BASE) as usize;
        return FontChoice::ALL
            .get(index)
            .copied()
            .map(MenuCommand::SetFont);
    }

    if (MENU_TAG_ALPHA_BASE..MENU_TAG_ALPHA_BASE + TRANSPARENCY_LEVELS.len() as isize)
        .contains(&tag)
    {
        let index = (tag - MENU_TAG_ALPHA_BASE) as usize;
        return TRANSPARENCY_LEVELS
            .get(index)
            .copied()
            .map(MenuCommand::SetTransparency);
    }

    if tag == MENU_TAG_SYNTAX_ON {
        return Some(MenuCommand::SetSyntaxHighlighting(true));
    }

    if tag == MENU_TAG_SYNTAX_OFF {
        return Some(MenuCommand::SetSyntaxHighlighting(false));
    }

    None
}

#[cfg(target_os = "macos")]
fn menu_item(title: &str, key: &str, target: id, action: Sel, tag: isize) -> id {
    unsafe {
        let title = NSString::alloc(nil).init_str(title);
        let key = NSString::alloc(nil).init_str(key);
        let item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(title, action, key);
        if target != nil {
            NSMenuItem::setTarget_(item, target);
        }
        let _: () = msg_send![item, setTag: tag];
        item
    }
}

#[cfg(target_os = "macos")]
fn transparency_label(level: f32) -> String {
    format!("{}%", (level * 100.0).round() as i32)
}

#[cfg(target_os = "macos")]
fn setup_status_item(cx: &mut App) {
    unsafe {
        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSVariableStatusItemLength);
        let button = status_item.button();
        let menu = NSMenu::new(nil);
        let handler_class = menu_action_handler_class();
        let handler: id = msg_send![handler_class, new];

        let show_item = menu_item(
            "Show Pasta",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SHOW,
        );
        menu.addItem_(show_item);

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let font_parent = menu_item("Font", "", handler, selector("menuAction:"), -1);
        let font_menu = NSMenu::new(nil);
        for (ix, choice) in FontChoice::ALL.into_iter().enumerate() {
            let tag = MENU_TAG_FONT_BASE + ix as isize;
            let item = menu_item(choice.label(), "", handler, selector("menuAction:"), tag);
            font_menu.addItem_(item);
        }
        font_parent.setSubmenu_(font_menu);
        menu.addItem_(font_parent);

        let transparency_parent =
            menu_item("Transparency", "", handler, selector("menuAction:"), -1);
        let transparency_menu = NSMenu::new(nil);
        for (ix, level) in TRANSPARENCY_LEVELS.into_iter().enumerate() {
            let tag = MENU_TAG_ALPHA_BASE + ix as isize;
            let label = transparency_label(level);
            let item = menu_item(&label, "", handler, selector("menuAction:"), tag);
            transparency_menu.addItem_(item);
        }
        transparency_parent.setSubmenu_(transparency_menu);
        menu.addItem_(transparency_parent);

        let syntax_parent = menu_item(
            "Syntax Highlighting",
            "",
            handler,
            selector("menuAction:"),
            -1,
        );
        let syntax_menu = NSMenu::new(nil);
        let syntax_on = menu_item(
            "Enabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SYNTAX_ON,
        );
        let syntax_off = menu_item(
            "Disabled",
            "",
            handler,
            selector("menuAction:"),
            MENU_TAG_SYNTAX_OFF,
        );
        syntax_menu.addItem_(syntax_on);
        syntax_menu.addItem_(syntax_off);
        syntax_parent.setSubmenu_(syntax_menu);
        menu.addItem_(syntax_parent);

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let close_item = menu_item(
            "Close Pasta",
            "q",
            handler,
            selector("menuAction:"),
            MENU_TAG_QUIT,
        );

        if button != nil {
            let title = NSString::alloc(nil).init_str("P");
            button.setTitle_(title);
        }

        menu.addItem_(close_item);
        status_item.setMenu_(menu);

        cx.set_global(StatusItemRegistration {
            _status_item: StrongPtr::retain(status_item as id),
            _menu: StrongPtr::retain(menu as id),
            _handler: StrongPtr::retain(handler as id),
        });
    }
}

#[cfg(target_os = "macos")]
fn setup_hotkey(cx: &mut App) {
    let manager = GlobalHotKeyManager::new().expect("failed to create global hotkey manager");
    let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
    manager
        .register(hotkey)
        .expect("failed to register Option+Space hotkey");

    cx.set_global(HotkeyRegistration {
        _manager: manager,
        hotkey_id: hotkey.id(),
    });
}

#[cfg(target_os = "macos")]
fn launcher_window_bounds(cx: &mut App) -> (Bounds<gpui::Pixels>, Option<gpui::DisplayId>) {
    let size = size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT));

    if let Some(display) = cx.primary_display() {
        let display_bounds = display.bounds();
        let origin = point(
            display_bounds.center().x - size.center().x,
            display_bounds.origin.y + px(TOP_OFFSET),
        );

        (Bounds { origin, size }, Some(display.id()))
    } else {
        (Bounds::centered(None, size, cx), None)
    }
}

#[cfg(target_os = "macos")]
fn create_launcher_window(cx: &mut App) -> WindowHandle<LauncherView> {
    let (bounds, display_id) = launcher_window_bounds(cx);
    let storage = cx.global::<StorageState>().storage.clone();
    let style = cx.global::<UiStyleState>().clone();

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            window_background: WindowBackgroundAppearance::Blurred,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            ..Default::default()
        },
        move |window, cx| {
            let storage = storage.clone();
            let style = style.clone();
            window.on_window_should_close(cx, |_, cx| {
                cx.hide();
                false
            });

            cx.new(move |cx| {
                let mut view = LauncherView::new(
                    storage.clone(),
                    style.family.clone(),
                    style.surface_alpha,
                    style.syntax_highlighting,
                );
                cx.observe_window_activation(window, |_view: &mut LauncherView, window, cx| {
                    if !window.is_window_active() {
                        _view.begin_close_transition(LauncherExitIntent::Hide);
                        cx.notify();
                    }
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

                view.refresh_items();
                view
            })
        },
    )
    .expect("failed to open launcher window")
}

#[cfg(target_os = "macos")]
fn load_embedded_ui_font(cx: &mut App) {
    let font_blobs: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/MesloLGSNF-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/MesloLGSNF-Bold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/MesloLGSNF-Italic.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/MesloLGSNF-BoldItalic.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/IosevkaTermNerdFont-Light.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/IBMPlexMono-Light.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Light.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceCodePro-Var.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/MonaspaceNeonFrozen-Light.ttf").as_slice()),
    ];

    if let Err(err) = cx.text_system().add_fonts(font_blobs) {
        eprintln!("warning: unable to load embedded Meslo font: {err}");
    }

    let family = resolve_font_family(cx, FontChoice::MesloLg).unwrap_or_else(|| "Menlo".into());

    cx.set_global(UiStyleState {
        family,
        surface_alpha: 0.72,
        syntax_highlighting: true,
    });
}

#[cfg(target_os = "macos")]
fn resolve_font_family(cx: &App, choice: FontChoice) -> Option<SharedString> {
    let all_names = cx.text_system().all_font_names();
    let all_normalized: Vec<String> = all_names
        .iter()
        .map(|name| normalize_font_name(name))
        .collect();

    for candidate in choice.candidates() {
        let candidate_normalized = normalize_font_name(candidate);
        if candidate_normalized.is_empty() {
            continue;
        }

        if let Some((ix, _)) = all_normalized
            .iter()
            .enumerate()
            .find(|(_, name)| *name == &candidate_normalized)
        {
            return Some(all_names[ix].clone().into());
        }
    }

    for candidate in choice.candidates() {
        let candidate_normalized = normalize_font_name(candidate);
        if candidate_normalized.is_empty() {
            continue;
        }

        if let Some((ix, _)) = all_normalized.iter().enumerate().find(|(_, name)| {
            name.contains(&candidate_normalized) || candidate_normalized.contains(*name)
        }) {
            return Some(all_names[ix].clone().into());
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn normalize_font_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

#[cfg(target_os = "macos")]
fn apply_style_to_open_window(cx: &mut App) {
    let style = cx.global::<UiStyleState>().clone();
    if let Some(window) = cx
        .try_global::<LauncherState>()
        .and_then(|state| state.window)
    {
        let _ = window.update(cx, |view, _window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            cx.notify();
        });
    }
}

#[cfg(target_os = "macos")]
fn handle_menu_command(command: MenuCommand, cx: &mut App) {
    match command {
        MenuCommand::ShowLauncher => show_launcher(cx),
        MenuCommand::QuitApp => {
            let mut should_terminate_now = true;
            if let Some(window) = cx
                .try_global::<LauncherState>()
                .and_then(|state| state.window)
            {
                if window.is_active(cx).unwrap_or(false) {
                    should_terminate_now = false;
                    let _ = window.update(cx, |view, _window, cx| {
                        view.begin_close_transition(LauncherExitIntent::Quit);
                        cx.notify();
                    });
                }
            }

            if should_terminate_now {
                unsafe {
                    let _: () = msg_send![NSApp(), terminate: nil];
                }
            }
        }
        MenuCommand::SetFont(choice) => {
            if let Some(family) = resolve_font_family(cx, choice) {
                cx.global_mut::<UiStyleState>().family = family;
                apply_style_to_open_window(cx);
            } else {
                let fallback = choice
                    .candidates()
                    .first()
                    .copied()
                    .unwrap_or_else(|| choice.label());
                cx.global_mut::<UiStyleState>().family = fallback.into();
                apply_style_to_open_window(cx);
                eprintln!(
                    "warning: requested font '{}' not resolved via text system; using fallback '{}'",
                    choice.label(),
                    fallback
                );
            }
        }
        MenuCommand::SetTransparency(alpha) => {
            cx.global_mut::<UiStyleState>().surface_alpha = alpha.clamp(0.45, 1.0);
            apply_style_to_open_window(cx);
        }
        MenuCommand::SetSyntaxHighlighting(enabled) => {
            cx.global_mut::<UiStyleState>().syntax_highlighting = enabled;
            apply_style_to_open_window(cx);
        }
    }
}

#[cfg(target_os = "macos")]
fn spawn_menu_command_listener(cx: &mut App, receiver: mpsc::Receiver<MenuCommand>) {
    cx.spawn(async move |cx| {
        loop {
            while let Ok(command) = receiver.try_recv() {
                let _ = cx.update(|cx| {
                    handle_menu_command(command, cx);
                });
            }

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn spawn_launcher_transition_loop(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let _ = cx.update(|cx| {
                if let Some(window) = cx
                    .try_global::<LauncherState>()
                    .and_then(|state| state.window)
                {
                    let _ = window.update(cx, |view, _window, cx| {
                        if !view.transition_running() {
                            return;
                        }

                        let maybe_exit = view.tick_transition();
                        cx.notify();

                        match maybe_exit {
                            Some(LauncherExitIntent::Hide) => cx.hide(),
                            Some(LauncherExitIntent::Quit) => unsafe {
                                let _: () = msg_send![NSApp(), terminate: nil];
                            },
                            None => {}
                        }
                    });
                }
            });

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn show_launcher(cx: &mut App) {
    cx.activate(true);
    let style = cx.global::<UiStyleState>().clone();

    let mut window = cx
        .try_global::<LauncherState>()
        .and_then(|state| state.window);
    if window.is_none() {
        let created = create_launcher_window(cx);
        cx.global_mut::<LauncherState>().window = Some(created);
        window = Some(created);
    }

    let Some(window) = window else { return };
    if window.is_active(cx).unwrap_or(false) {
        let _ = window.update(cx, |view, _window, cx| {
            view.begin_close_transition(LauncherExitIntent::Hide);
            cx.notify();
        });
        return;
    }

    if window
        .update(cx, |view, window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            view.reset_for_show();
            view.begin_open_transition();
            cx.notify();
            window.activate_window();
        })
        .is_err()
    {
        let created = create_launcher_window(cx);
        cx.global_mut::<LauncherState>().window = Some(created);
        let _ = created.update(cx, |view, window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            view.reset_for_show();
            view.begin_open_transition();
            cx.notify();
            window.activate_window();
        });
    }
}

#[cfg(target_os = "macos")]
fn spawn_hotkey_listener(cx: &mut App) {
    let receiver = GlobalHotKeyEvent::receiver().clone();

    cx.spawn(async move |cx| {
        loop {
            while let Ok(event) = receiver.try_recv() {
                let is_trigger = cx
                    .try_read_global::<HotkeyRegistration, _>(|registration, _| {
                        event.id == registration.hotkey_id && event.state == HotKeyState::Pressed
                    })
                    .unwrap_or(false);

                if is_trigger {
                    let _ = cx.update(|cx| {
                        show_launcher(cx);
                    });
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn spawn_clipboard_watcher(cx: &mut App) {
    let storage = cx.global::<StorageState>().storage.clone();

    cx.spawn(async move |cx| {
        let mut last_change_count = clipboard_change_count();
        loop {
            let current_change_count = clipboard_change_count();
            if current_change_count != last_change_count {
                last_change_count = current_change_count;

                if let Some(text) = read_clipboard_text() {
                    let inserted = storage.upsert_clipboard_item(&text).unwrap_or(false);
                    if inserted {
                        let _ = cx.update(|cx| {
                            if let Some(window) = cx
                                .try_global::<LauncherState>()
                                .and_then(|state| state.window)
                            {
                                let _ = window.update(cx, |view, _window, cx| {
                                    view.refresh_items();
                                    cx.notify();
                                });
                            }
                        });
                    }
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn clipboard_change_count() -> i64 {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard(nil);
        pasteboard.changeCount() as i64
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard_text() -> Option<String> {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard(nil);
        let text = pasteboard.stringForType(NSPasteboardTypeString);
        if text == nil {
            return None;
        }

        let utf8_ptr = text.UTF8String();
        if utf8_ptr.is_null() {
            return None;
        }

        Some(CStr::from_ptr(utf8_ptr).to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
fn ensure_launch_agent_registered() {
    if let Err(err) = install_launch_agent_plist() {
        eprintln!("warning: unable to configure launch-at-login: {err}");
    }
}

#[cfg(target_os = "macos")]
fn install_launch_agent_plist() -> std::io::Result<()> {
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(());
    };

    let launch_agents_dir = home_dir.join("Library").join("LaunchAgents");
    fs::create_dir_all(&launch_agents_dir)?;

    let plist_path = launch_agents_dir.join(format!("{LAUNCH_AGENT_LABEL}.plist"));
    let executable_path = env::current_exe()?;
    let executable = xml_escape(&executable_path.to_string_lossy());

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>LimitLoadToSessionType</key>
    <array>
        <string>Aqua</string>
    </array>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        executable = executable
    );

    let should_write = match fs::read_to_string(&plist_path) {
        Ok(existing) => existing != plist,
        Err(_) => true,
    };

    if should_write {
        fs::write(&plist_path, plist)?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn main() {
    Application::new().run(|cx: &mut App| {
        ensure_launch_agent_registered();

        let (menu_tx, menu_rx) = mpsc::channel::<MenuCommand>();
        let _ = MENU_COMMAND_TX.set(menu_tx);

        let storage = Arc::new(
            ClipboardStorage::bootstrap("PastaClipboard")
                .expect("failed to initialize clipboard storage"),
        );
        if let Some(initial_text) = read_clipboard_text() {
            let _ = storage.upsert_clipboard_item(&initial_text);
        }

        cx.set_global(StorageState {
            storage: storage.clone(),
        });
        load_embedded_ui_font(cx);

        let window = create_launcher_window(cx);
        cx.set_global(LauncherState {
            window: Some(window),
        });
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
