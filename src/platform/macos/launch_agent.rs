#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use std::path::Path;

// Only a binary living inside a .app bundle under /Applications or ~/Applications
// is considered a stable install location. We refuse to auto-pin a LaunchAgent
// at a path under target/debug, target/release, /tmp, etc., because doing so
// would silently break the user's login item on `cargo clean`.
#[cfg(target_os = "macos")]
fn is_stable_install_location(exe: &Path) -> bool {
    let s = exe.to_string_lossy();
    if !s.contains(".app/Contents/MacOS/") {
        return false;
    }
    if s.starts_with("/Applications/") {
        return true;
    }
    if let Some(home) = dirs::home_dir() {
        let prefix = format!("{}/Applications/", home.to_string_lossy());
        if s.starts_with(&prefix) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn launch_agent_plist_path() -> Option<std::path::PathBuf> {
    let home_dir = dirs::home_dir()?;
    Some(
        home_dir
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist")),
    )
}

#[cfg(target_os = "macos")]
fn render_launch_agent_plist(executable_path: &Path) -> String {
    let executable = xml_escape(&executable_path.to_string_lossy());
    format!(
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
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_agent_is_installed() -> bool {
    launch_agent_plist_path().is_some_and(|path| path.exists())
}

/// Called once on startup. If a LaunchAgent plist already exists AND the current
/// executable lives at a stable install location, keep it pointed at the current
/// binary so app updates keep working. Never create a new plist here — that is
/// reserved for an explicit user opt-in via the menu.
#[cfg(target_os = "macos")]
pub(crate) fn ensure_launch_agent_registered() {
    let Some(plist_path) = launch_agent_plist_path() else {
        return;
    };
    if !plist_path.exists() {
        return;
    }
    let Ok(executable_path) = env::current_exe() else {
        return;
    };
    if !is_stable_install_location(&executable_path) {
        // Running from a transient location (e.g. target/debug). Leave the
        // existing plist alone so `cargo run` during development does not
        // rewrite the user's installed login item.
        return;
    }
    if let Err(err) = write_launch_agent_plist(&plist_path, &executable_path) {
        eprintln!("warning: unable to refresh launch-at-login plist: {err}");
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn install_launch_agent() -> std::io::Result<()> {
    let Some(plist_path) = launch_agent_plist_path() else {
        return Err(std::io::Error::other("home directory unavailable"));
    };
    let executable_path = env::current_exe()?;
    if !is_stable_install_location(&executable_path) {
        return Err(std::io::Error::other(
            "Pasta must be running from /Applications or ~/Applications to enable launch at login",
        ));
    }
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_launch_agent_plist(&plist_path, &executable_path)
}

#[cfg(target_os = "macos")]
pub(crate) fn uninstall_launch_agent() -> std::io::Result<()> {
    let Some(plist_path) = launch_agent_plist_path() else {
        return Ok(());
    };
    match fs::remove_file(&plist_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(target_os = "macos")]
fn write_launch_agent_plist(plist_path: &Path, executable_path: &Path) -> std::io::Result<()> {
    let plist = render_launch_agent_plist(executable_path);
    let should_write = match fs::read_to_string(plist_path) {
        Ok(existing) => existing != plist,
        Err(_) => true,
    };
    if should_write {
        fs::write(plist_path, plist)?;
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
