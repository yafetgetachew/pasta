use std::ops::Range;
use std::sync::OnceLock;

use gpui::{HighlightStyle, StyledText};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

use super::LanguageTag;

struct UiSyntaxHighlighter {
    syntax_set: SyntaxSet,
    dark_theme: Theme,
    light_theme: Theme,
}

pub(crate) fn syntax_styled_text(
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

pub(crate) fn syntax_highlights(
    text: &str,
    language: LanguageTag,
    dark: bool,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if text.is_empty() {
        return Vec::new();
    }

    let Some(highlighter) = ui_syntax_highlighter() else {
        return Vec::new();
    };
    let syntax = syntect_syntax_for_language(&highlighter.syntax_set, language, text)
        .unwrap_or_else(|| highlighter.syntax_set.find_syntax_plain_text());
    let theme = if dark {
        &highlighter.dark_theme
    } else {
        &highlighter.light_theme
    };
    let mut line_highlighter = HighlightLines::new(syntax, theme);

    let mut highlights = Vec::new();
    let mut offset = 0usize;
    for line in LinesWithEndings::from(text) {
        let spans = match line_highlighter.highlight_line(line, &highlighter.syntax_set) {
            Ok(spans) => spans,
            Err(err) => {
                eprintln!("warning: syntect highlight failed: {err}");
                return Vec::new();
            }
        };

        let mut span_start = offset;
        for (style, span_text) in spans {
            let span_end = span_start + span_text.len();
            if span_start < span_end
                && text.is_char_boundary(span_start)
                && text.is_char_boundary(span_end)
            {
                highlights.push((span_start..span_end, syntect_style_to_highlight(style)));
            }
            span_start = span_end;
        }
        offset += line.len();
    }

    highlights
}

fn syntect_style_to_highlight(style: SyntectStyle) -> HighlightStyle {
    let rgba = gpui::Rgba {
        r: style.foreground.r as f32 / 255.0,
        g: style.foreground.g as f32 / 255.0,
        b: style.foreground.b as f32 / 255.0,
        a: style.foreground.a as f32 / 255.0,
    };
    let hsla: gpui::Hsla = rgba.into();
    HighlightStyle::color(hsla)
}

fn ui_syntax_highlighter() -> Option<&'static UiSyntaxHighlighter> {
    static SYNTAX_HIGHLIGHTER: OnceLock<Option<UiSyntaxHighlighter>> = OnceLock::new();
    SYNTAX_HIGHLIGHTER
        .get_or_init(|| {
            let syntax_set = SyntaxSet::load_defaults_newlines();
            let theme_set = ThemeSet::load_defaults();

            let dark_theme = select_syntect_theme(
                &theme_set,
                &["base16-ocean.dark", "Solarized (dark)", "Monokai Extended"],
            )?;
            let light_theme = select_syntect_theme(
                &theme_set,
                &["InspiredGitHub", "Solarized (light)", "base16-ocean.light"],
            )?;

            Some(UiSyntaxHighlighter {
                syntax_set,
                dark_theme,
                light_theme,
            })
        })
        .as_ref()
}

fn select_syntect_theme(theme_set: &ThemeSet, preferred_names: &[&str]) -> Option<Theme> {
    preferred_names
        .iter()
        .find_map(|name| theme_set.themes.get(*name).cloned())
        .or_else(|| theme_set.themes.values().next().cloned())
}

fn syntect_syntax_for_language<'a>(
    syntax_set: &'a SyntaxSet,
    language: LanguageTag,
    text: &str,
) -> Option<&'a SyntaxReference> {
    let extension = match language {
        LanguageTag::Bash => Some("sh"),
        LanguageTag::Rust => Some("rs"),
        LanguageTag::Python => Some("py"),
        LanguageTag::TypeScript => Some("ts"),
        LanguageTag::JavaScript => Some("js"),
        LanguageTag::Go => Some("go"),
        LanguageTag::Java => Some("java"),
        LanguageTag::Cpp => Some("cpp"),
        LanguageTag::Sql => Some("sql"),
        LanguageTag::Json => Some("json"),
        LanguageTag::Yaml => Some("yaml"),
        LanguageTag::Html => Some("html"),
        LanguageTag::Css => Some("css"),
        LanguageTag::Markdown => Some("md"),
        LanguageTag::Toml => Some("toml"),
        LanguageTag::Code => None,
    };

    extension
        .and_then(|ext| syntax_set.find_syntax_by_extension(ext))
        .or_else(|| syntax_set.find_syntax_by_first_line(text))
}
