#[cfg(target_os = "macos")]
use super::LanguageTag;
#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) dark: bool,
    pub(crate) window_bg: gpui::Rgba,
    pub(crate) window_border: gpui::Rgba,
    pub(crate) title_text: gpui::Rgba,
    pub(crate) query_placeholder: gpui::Rgba,
    pub(crate) query_active: gpui::Rgba,
    pub(crate) muted_text: gpui::Rgba,
    pub(crate) list_divider: gpui::Rgba,
    pub(crate) row_text: gpui::Rgba,
    pub(crate) row_meta_text: gpui::Rgba,
    pub(crate) row_hover_bg: gpui::Rgba,
    pub(crate) selected_bg: gpui::Rgba,
    pub(crate) selected_border: gpui::Rgba,
}

#[cfg(target_os = "macos")]
pub(crate) fn palette_for(appearance: WindowAppearance, surface_alpha: f32) -> Palette {
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
            window_bg: rgba(0xfffffff2),
            window_border: rgba(0x0f172a1f),
            title_text: rgba(0x0f172ad9),
            query_placeholder: rgba(0x64748bb8),
            query_active: rgba(0x020617f2),
            muted_text: rgba(0x334155c4),
            list_divider: rgba(0x33415517),
            row_text: rgba(0x020617eb),
            row_meta_text: rgba(0x475569ab),
            row_hover_bg: rgba(0x1d4ed818),
            selected_bg: rgba(0x2563eb3d),
            selected_border: rgba(0x1d4ed88f),
        }
    };

    let alpha_scale = surface_alpha.clamp(0.45, 1.0);
    palette.window_bg = scale_alpha(palette.window_bg, alpha_scale);
    palette.window_border = scale_alpha(palette.window_border, alpha_scale);
    palette.list_divider = scale_alpha(palette.list_divider, alpha_scale);
    palette.row_hover_bg = scale_alpha(palette.row_hover_bg, alpha_scale);
    palette.selected_bg = scale_alpha(palette.selected_bg, alpha_scale);
    palette.selected_border = scale_alpha(palette.selected_border, alpha_scale);
    if !palette.dark {
        // Keep selected rows visible even when the user lowers panel transparency.
        palette.selected_bg.a = palette.selected_bg.a.max(0.16);
        palette.selected_border.a = palette.selected_border.a.max(0.42);
    }

    palette
}

#[cfg(target_os = "macos")]
pub(crate) fn scale_alpha(color: gpui::Rgba, scale: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: (color.a * scale).clamp(0.0, 1.0),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn type_color(item_type: ClipboardItemType, dark: bool) -> gpui::Hsla {
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
fn push_unique_chip(chips: &mut Vec<String>, label: &str) {
    if !chips.iter().any(|existing| existing == label) {
        chips.push(label.to_owned());
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn visible_tag_chips(
    item_type: ClipboardItemType,
    language: Option<LanguageTag>,
    tags: &[String],
) -> Vec<String> {
    let mut chips = Vec::new();

    let has = |needle: &str| tags.iter().any(|tag| tag.eq_ignore_ascii_case(needle));

    if item_type == ClipboardItemType::Text {
        push_unique_chip(&mut chips, "TEXT");
    } else {
        push_unique_chip(&mut chips, item_type.label());
    }
    if let Some(language) = language {
        push_unique_chip(&mut chips, language.label());
    }

    if chips.len() < MAX_VISIBLE_TAG_CHIPS {
        for raw in tags {
            let lower = raw.to_ascii_lowercase();
            if lower.is_empty()
                || matches!(
                    lower.as_str(),
                    "text"
                        | "code"
                        | "command"
                        | "password"
                        | "shell"
                        | "singleline"
                        | "multiline"
                        | "long"
                        | "url"
                        | "path"
                        | "env"
                        | "sensitive"
                )
                || lower.starts_with("type:")
                || lower.starts_with("lang:")
            {
                continue;
            }

            let normalized = raw.to_ascii_uppercase();
            push_unique_chip(&mut chips, &normalized);
            if chips.len() >= MAX_VISIBLE_TAG_CHIPS {
                break;
            }
        }
    }

    if has("sensitive") {
        push_unique_chip(&mut chips, "SECRET");
    }
    if has("env") {
        push_unique_chip(&mut chips, "ENV");
    }
    if has("path") {
        push_unique_chip(&mut chips, "PATH");
    }
    if has("url") {
        push_unique_chip(&mut chips, "URL");
    }
    if has("long") {
        push_unique_chip(&mut chips, "LONG");
    }
    if has("multiline") {
        push_unique_chip(&mut chips, "MULTI");
    }

    chips.truncate(MAX_VISIBLE_TAG_CHIPS);
    chips
}

#[cfg(target_os = "macos")]
pub(crate) fn tag_chip_color(label: &str, dark: bool) -> gpui::Hsla {
    if label.starts_with("OPEN ") {
        if dark {
            return rgb(0x4ade80).into();
        }
        return rgb(0x15803d).into();
    }
    if label.starts_with("P:") {
        if dark {
            return rgb(0x67e8f9).into();
        }
        return rgb(0x0e7490).into();
    }

    match label {
        "LOCKED" => {
            if dark {
                rgb(0xfb7185).into()
            } else {
                rgb(0xbe123c).into()
            }
        }
        "TEXT" => type_color(ClipboardItemType::Text, dark),
        "CODE" => type_color(ClipboardItemType::Code, dark),
        "CMD" => type_color(ClipboardItemType::Command, dark),
        "PASS" | "SECRET" => type_color(ClipboardItemType::Password, dark),
        "BASH" => language_color(LanguageTag::Bash, dark),
        "RUST" => language_color(LanguageTag::Rust, dark),
        "PY" => language_color(LanguageTag::Python, dark),
        "TS" => language_color(LanguageTag::TypeScript, dark),
        "JS" => language_color(LanguageTag::JavaScript, dark),
        "GO" => language_color(LanguageTag::Go, dark),
        "JAVA" => language_color(LanguageTag::Java, dark),
        "C++" => language_color(LanguageTag::Cpp, dark),
        "SQL" => language_color(LanguageTag::Sql, dark),
        "JSON" => language_color(LanguageTag::Json, dark),
        "YAML" => language_color(LanguageTag::Yaml, dark),
        "HTML" => language_color(LanguageTag::Html, dark),
        "CSS" => language_color(LanguageTag::Css, dark),
        "MD" => language_color(LanguageTag::Markdown, dark),
        "TOML" => language_color(LanguageTag::Toml, dark),
        "PARAM" => {
            if dark {
                rgb(0x93c5fd).into()
            } else {
                rgb(0x1d4ed8).into()
            }
        }
        "INFO" => {
            if dark {
                rgb(0x7dd3fc).into()
            } else {
                rgb(0x0369a1).into()
            }
        }
        "ENV" => {
            if dark {
                rgb(0xa78bfa).into()
            } else {
                rgb(0x6d28d9).into()
            }
        }
        "PATH" => {
            if dark {
                rgb(0x93c5fd).into()
            } else {
                rgb(0x1d4ed8).into()
            }
        }
        "URL" => {
            if dark {
                rgb(0x5eead4).into()
            } else {
                rgb(0x0f766e).into()
            }
        }
        "MULTI" => {
            if dark {
                rgb(0xfde047).into()
            } else {
                rgb(0xa16207).into()
            }
        }
        "LONG" => {
            if dark {
                rgb(0xfdba74).into()
            } else {
                rgb(0xc2410c).into()
            }
        }
        _ => {
            if dark {
                rgb(0xd1d5db).into()
            } else {
                rgb(0x4b5563).into()
            }
        }
    }
}
