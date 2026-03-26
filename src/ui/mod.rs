#[cfg(target_os = "macos")]
mod language;
#[cfg(target_os = "macos")]
mod palette;
#[cfg(target_os = "macos")]
mod preview;
#[cfg(target_os = "macos")]
mod syntax;

#[cfg(target_os = "macos")]
pub(crate) use language::{LanguageTag, detect_language, language_color};
#[cfg(target_os = "macos")]
pub(crate) use palette::{Palette, palette_for, scale_alpha, tag_chip_color, visible_tag_chips};
#[cfg(target_os = "macos")]
pub(crate) use preview::{format_timestamp, masked_secret_preview, preview_content};
#[cfg(all(target_os = "macos", test))]
pub(crate) use syntax::syntax_highlights;
#[cfg(target_os = "macos")]
pub(crate) use syntax::syntax_styled_text;
