mod language;
mod palette;
mod preview;
mod syntax;

pub(crate) use language::{LanguageTag, detect_language, language_color};
pub(crate) use palette::{Palette, palette_for, scale_alpha, tag_chip_color, visible_tag_chips};
pub(crate) use preview::{
    bounded_preview_content, expanded_preview_content, format_timestamp, format_timestamp_detail,
    masked_secret_preview, preview_content,
};
#[cfg(test)]
pub(crate) use syntax::syntax_highlights;
pub(crate) use syntax::syntax_styled_text;
