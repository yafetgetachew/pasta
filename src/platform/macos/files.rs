#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn choose_path_with_script(script: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(PathBuf::from(trimmed))
}

#[cfg(target_os = "macos")]
pub(crate) fn choose_bowl_export_path(prompt: &str, default_name: &str) -> Option<PathBuf> {
    let prompt = escape_applescript_string(prompt);
    let default_name = escape_applescript_string(default_name);
    let script = format!(
        "POSIX path of (choose file name with prompt \"{prompt}\" default name \"{default_name}\")"
    );
    let mut path = choose_path_with_script(&script)?;
    if path.extension().is_none() {
        path.set_extension("yaml");
    }
    Some(path)
}

#[cfg(target_os = "macos")]
pub(crate) fn choose_bowl_import_path(prompt: &str) -> Option<PathBuf> {
    let prompt = escape_applescript_string(prompt);
    let script = format!("POSIX path of (choose file with prompt \"{prompt}\")");
    choose_path_with_script(&script)
}
