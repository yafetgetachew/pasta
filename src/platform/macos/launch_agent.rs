#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
pub(crate) fn ensure_launch_agent_registered() {
    if let Err(err) = install_launch_agent_plist() {
        eprintln!("warning: unable to configure launch-at-login: {err}");
    }
}

#[cfg(target_os = "macos")]
fn install_launch_agent_plist() -> std::io::Result<()> {
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(());
    };

    let launch_agents_dir = home_dir.join("Library").join("LaunchAgents");
    fs::create_dir_all(&launch_agents_dir)?;

    let plist_path = launch_agents_dir.join(format!("{LAUNCH_AGENT_LABEL}.plist"));
    let executable_path = env::current_exe()?;
    let executable = xml_escape(&executable_path.to_string_lossy());

    let plist = format!(
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
    );

    let should_write = match fs::read_to_string(&plist_path) {
        Ok(existing) => existing != plist,
        Err(_) => true,
    };

    if should_write {
        fs::write(&plist_path, plist)?;
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
