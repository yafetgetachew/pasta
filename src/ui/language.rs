use crate::*;

#[derive(Clone, Copy)]
pub(crate) enum LanguageTag {
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

impl LanguageTag {
    pub(crate) fn label(&self) -> &'static str {
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

pub(crate) fn detect_language(item_type: ClipboardItemType, content: &str) -> Option<LanguageTag> {
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

pub(crate) fn language_color(language: LanguageTag, dark: bool) -> gpui::Hsla {
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
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_json(text: &str, lower: &str) -> bool {
    let trimmed = text.trim();
    let wrapped = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    wrapped && lower.contains(':') && trimmed.contains('"')
}

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
