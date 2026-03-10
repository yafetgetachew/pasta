#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn masked_secret_preview(content: &str) -> String {
    let width = content.chars().count().clamp(8, 32);
    format!(
        "{}  (hidden secret, press Enter or ⌘R to reveal)",
        "•".repeat(width)
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn preview_content(content: &str) -> String {
    let wrapped = expanded_preview_content(content);
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
pub(crate) fn expanded_preview_content(content: &str) -> String {
    let normalized = content.replace('\r', "").replace('\t', "    ");
    wrap_long_words(&normalized, PREVIEW_WRAP_RUN)
}

#[cfg(target_os = "macos")]
pub(crate) fn preview_would_truncate(content: &str) -> bool {
    expanded_preview_content(content).lines().count() > PREVIEW_LINE_LIMIT
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
pub(crate) fn format_timestamp(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "now".to_owned())
}
