#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_change_count() -> i64 {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard(nil);
        pasteboard.changeCount()
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
pub(crate) struct ClipboardSnapshot {
    pub(crate) text: String,
    pub(crate) is_concealed: bool,
    pub(crate) is_transient: bool,
}

#[cfg(target_os = "macos")]
pub(crate) fn read_clipboard_snapshot() -> Option<ClipboardSnapshot> {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard(nil);
        let type_names = pasteboard_type_names(pasteboard);
        let type_names_lower: Vec<String> = type_names
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect();

        // When files are copied (e.g. Cmd+C in Finder), prefer the full file path(s)
        // over the plain string, which is typically just the filename.
        let file_paths = read_file_urls_from_pasteboard(pasteboard, &type_names_lower);
        let text = if let Some(paths) = file_paths {
            paths
        } else {
            let ns_text = pasteboard.stringForType(NSPasteboardTypeString);
            if ns_text == nil {
                return None;
            }
            let utf8_ptr = ns_text.UTF8String();
            if utf8_ptr.is_null() {
                return None;
            }
            CStr::from_ptr(utf8_ptr).to_string_lossy().into_owned()
        };

        let is_transient = type_names_lower.iter().any(|kind| {
            kind == "org.nspasteboard.transienttype"
                || kind.contains("org.nspasteboard.transienttype")
        });
        let is_concealed = type_names_lower.iter().any(|kind| {
            kind == "org.nspasteboard.concealedtype"
                || kind.contains("org.nspasteboard.concealedtype")
                || kind.contains("com.agilebits.onepassword")
                || kind.contains("onepassword")
                || kind.contains("bitwarden")
        });

        Some(ClipboardSnapshot {
            text,
            is_concealed,
            is_transient,
        })
    }
}

/// Reads file URLs from the pasteboard when files are copied (e.g. Cmd+C in Finder).
/// Returns the file path(s) as a newline-separated string, or None if no file URLs are present.
///
/// Finder often uses file reference URLs (`file:///.file/id=...`) rather than literal paths,
/// so we resolve them via NSURL which handles both forms.
#[cfg(target_os = "macos")]
fn read_file_urls_from_pasteboard(pasteboard: id, type_names_lower: &[String]) -> Option<String> {
    let has_file_urls = type_names_lower.iter().any(|kind| {
        kind == "public.file-url" || kind.contains("public.file-url")
    });

    if !has_file_urls {
        return None;
    }

    unsafe {
        // Read all items from the pasteboard — each file is a separate pasteboard item.
        let items: id = msg_send![pasteboard, pasteboardItems];
        if items == nil {
            return None;
        }

        let count: usize = msg_send![items, count];
        if count == 0 {
            return None;
        }

        let file_url_type = NSString::alloc(nil).init_str("public.file-url");
        let nsurl_class = class!(NSURL);
        let mut paths = Vec::with_capacity(count);

        for ix in 0..count {
            let item: id = msg_send![items, objectAtIndex: ix];
            if item == nil {
                continue;
            }
            let url_string: id = msg_send![item, stringForType: file_url_type];
            if url_string == nil {
                continue;
            }

            // Use NSURL to resolve the URL — this handles file reference URLs
            // (file:///.file/id=...) and regular file URLs (file:///Users/...) alike.
            let nsurl: id = msg_send![nsurl_class, URLWithString: url_string];
            if nsurl == nil {
                continue;
            }
            let ns_path: id = msg_send![nsurl, path];
            if ns_path == nil {
                continue;
            }
            let utf8_ptr = ns_path.UTF8String();
            if utf8_ptr.is_null() {
                continue;
            }
            let path = CStr::from_ptr(utf8_ptr).to_string_lossy().into_owned();
            if !path.is_empty() {
                paths.push(path);
            }
        }

        if paths.is_empty() {
            return None;
        }

        Some(paths.join("\n"))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn read_clipboard_text() -> Option<String> {
    read_clipboard_snapshot().map(|snapshot| snapshot.text)
}

#[cfg(target_os = "macos")]
fn pasteboard_type_names(pasteboard: id) -> Vec<String> {
    unsafe {
        let types: id = msg_send![pasteboard, types];
        if types == nil {
            return Vec::new();
        }

        let count: usize = msg_send![types, count];
        let mut output = Vec::with_capacity(count);
        for ix in 0..count {
            let item: id = msg_send![types, objectAtIndex: ix];
            if item == nil {
                continue;
            }
            let utf8_ptr = item.UTF8String();
            if utf8_ptr.is_null() {
                continue;
            }
            output.push(CStr::from_ptr(utf8_ptr).to_string_lossy().into_owned());
        }

        output
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
pub(crate) fn parse_custom_tags_input(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();

    for token in input.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            tags.push(trimmed.to_owned());
        }
    }

    tags
}

#[cfg(target_os = "macos")]
pub(crate) fn show_macos_notification(title: &str, body: &str) {
    let title = escape_applescript_string(title);
    let body = escape_applescript_string(body);
    let script = format!("display notification \"{body}\" with title \"{title}\"");
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
}

#[cfg(target_os = "macos")]
pub(crate) fn write_clipboard_text(value: &str) {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard(nil);
        let _: usize = msg_send![pasteboard, clearContents];
        let ns = NSString::alloc(nil).init_str(value);
        let _: usize = msg_send![pasteboard, setString: ns forType: NSPasteboardTypeString];
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_text_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn process_secret_autoclear(cx: &mut App) {
    let pending = cx
        .try_global::<AutoClearState>()
        .and_then(|state| state.pending.clone());
    let Some(pending) = pending else { return };
    if Instant::now() < pending.due_at {
        return;
    }

    let should_clear = read_clipboard_text()
        .map(|current| clipboard_text_hash(&current) == pending.expected_hash)
        .unwrap_or(false);
    if should_clear {
        write_clipboard_text("");
    }

    cx.global_mut::<AutoClearState>().pending = None;
}

#[cfg(target_os = "macos")]
pub(crate) fn should_ignore_self_clipboard_write(cx: &mut App, text: &str) -> bool {
    let pending = cx
        .try_global::<SelfClipboardWriteState>()
        .and_then(|state| state.pending.clone());
    let Some(pending) = pending else { return false };

    if Instant::now() > pending.due_at {
        cx.global_mut::<SelfClipboardWriteState>().pending = None;
        return false;
    }

    if clipboard_text_hash(text) == pending.expected_hash {
        cx.global_mut::<SelfClipboardWriteState>().pending = None;
        return true;
    }

    false
}
