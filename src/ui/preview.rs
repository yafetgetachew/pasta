use crate::*;
use chrono::{DateTime, Local};

pub(crate) fn masked_secret_preview(content: &str) -> String {
    let width = content.chars().count().clamp(8, 32);
    format!("{}  (secret, ⌘R reveal)", "•".repeat(width))
}

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

pub(crate) fn expanded_preview_content(content: &str) -> String {
    let normalized = content.replace('\r', "").replace('\t', "    ");
    wrap_long_words(&normalized, PREVIEW_WRAP_RUN)
}

pub(crate) fn bounded_preview_content(content: &str, limit: usize) -> (String, bool) {
    if content.len() <= limit {
        return (content.to_owned(), false);
    }

    let mut end = limit.min(content.len());
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }

    let mut bounded = content[..end].to_owned();
    bounded.push_str("\n\n... Preview shortened for speed.");
    (bounded, true)
}

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

pub(crate) fn format_timestamp(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "now".to_owned())
}

pub(crate) fn format_timestamp_detail(timestamp: &str) -> String {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|value| {
            value
                .with_timezone(&Local)
                .format("%b %-d, %Y • %-I:%M %p")
                .to_string()
        })
        .unwrap_or_else(|_| timestamp.to_owned())
}
