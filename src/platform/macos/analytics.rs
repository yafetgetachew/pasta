#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(target_os = "macos")]
use std::process::{Command, Output, Stdio};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Every subprocess this module spawns (ioreg, sw_vers, curl) must finish inside
// this window or be killed. Analytics is a side channel; it is never allowed to
// leak a thread waiting on a hung child.
#[cfg(target_os = "macos")]
const SUBPROCESS_MAX_DURATION: Duration = Duration::from_secs(5);

// Placeholder analytics endpoint. The `.invalid` TLD is reserved by RFC 2606 and
// will never resolve, so a misconfigured build can never leak data to a real host.
#[cfg(target_os = "macos")]
const ANALYTICS_ENDPOINT: &str = "https://analytics.pasta.invalid/v1/events";

#[cfg(target_os = "macos")]
const FINGERPRINT_SALT: &[u8] = b"pasta-launcher/v1";

#[cfg(target_os = "macos")]
const HEARTBEAT_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[cfg(target_os = "macos")]
#[derive(Debug, Serialize, Deserialize, Default)]
struct AnalyticsState {
    last_sent_epoch: Option<u64>,
}

// Baseline fields (install_id, event, app_version, clipboard_count) are always
// transmitted — they are the minimum required to distinguish installs and track
// the single headline metric. Fields wrapped in Option + skip_serializing_if are
// only attached when the user has opted in to detailed analytics from the menu.
#[cfg(target_os = "macos")]
#[derive(Debug, Serialize)]
struct AnalyticsEvent<'a> {
    install_id: &'a str,
    event: &'a str,
    app_version: &'a str,
    clipboard_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    os: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

#[cfg(target_os = "macos")]
fn analytics_state_path() -> Option<PathBuf> {
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::home_dir)?;
    let directory = base.join("PastaClipboard");
    fs::create_dir_all(&directory).ok()?;
    Some(directory.join("analytics-state.json"))
}

#[cfg(target_os = "macos")]
fn load_analytics_state() -> AnalyticsState {
    let Some(path) = analytics_state_path() else {
        return AnalyticsState::default();
    };
    let Ok(data) = fs::read_to_string(&path) else {
        return AnalyticsState::default();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn save_analytics_state(state: &AnalyticsState) {
    let Some(path) = analytics_state_path() else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&path, serialized);
    }
}

// Polls an already-spawned child until it exits or the deadline elapses, killing
// it on timeout. std has no built-in timeout API for Command, and analytics is
// never allowed to leak a thread on a hung subprocess, so we supply our own.
#[cfg(target_os = "macos")]
fn run_bounded(mut cmd: Command, max: Duration) -> Option<Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + max;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

// Shells out to `ioreg` rather than linking IOKit directly — keeps the dependency
// footprint zero and avoids wiring unsafe FFI for a once-per-day read.
#[cfg(target_os = "macos")]
fn mac_serial_number() -> Option<String> {
    let mut cmd = Command::new("/usr/sbin/ioreg");
    cmd.args(["-rd1", "-c", "IOPlatformExpertDevice"]);
    let output = run_bounded(cmd, SUBPROCESS_MAX_DURATION)?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if !line.contains("IOPlatformSerialNumber") {
            continue;
        }
        let Some(eq_idx) = line.rfind('=') else {
            continue;
        };
        let raw = line[eq_idx + 1..].trim().trim_matches('"');
        if !raw.is_empty() {
            return Some(raw.to_owned());
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn install_fingerprint() -> Option<String> {
    let serial = mac_serial_number()?;
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_SALT);
    hasher.update(b":");
    hasher.update(serial.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Some(format!("sha256:{hex}"))
}

#[cfg(target_os = "macos")]
fn macos_product_version() -> String {
    let mut cmd = Command::new("/usr/bin/sw_vers");
    cmd.arg("-productVersion");
    run_bounded(cmd, SUBPROCESS_MAX_DURATION)
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(target_os = "macos")]
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Curl's own --max-time guards the transfer; run_bounded is the belt-and-
// suspenders outer deadline (12s) in case curl itself wedges before honouring
// its timer. Either way the thread is never stuck.
#[cfg(target_os = "macos")]
fn post_event_via_curl(payload: &str) {
    let mut cmd = Command::new("/usr/bin/curl");
    cmd.args([
        "-fsS",
        "--max-time",
        "10",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
        "-d",
        payload,
        ANALYTICS_ENDPOINT,
    ]);
    // Silent on failure: analytics must never disrupt the app. The `.invalid`
    // endpoint always fails DNS, which is expected until a real host is wired up.
    let _ = run_bounded(cmd, Duration::from_secs(12));
}

// Baseline heartbeat (install_id + app_version + clipboard_count) runs for every
// user — opt-out is not offered for those three metrics. `detailed_opt_in` only
// governs whether the optional fields (os, os_version, timestamp) ride along.
//
// The caller does no I/O: state load, throttle check, subprocess execution and
// state save all happen inside the detached worker thread. The caller thread
// incurs a single Arc clone and the fixed cost of thread creation.
#[cfg(target_os = "macos")]
pub(crate) fn maybe_send_heartbeat(storage: Arc<ClipboardStorage>, detailed_opt_in: bool) {
    spawn_heartbeat(storage, detailed_opt_in, true);
}

// Unthrottled variant used when the user explicitly flips the detailed-analytics
// toggle in the menu. Bypasses the 24h throttle so the server sees the updated
// detail level immediately, rather than silently waiting out the window.
#[cfg(target_os = "macos")]
pub(crate) fn send_heartbeat_now(storage: Arc<ClipboardStorage>, detailed_opt_in: bool) {
    spawn_heartbeat(storage, detailed_opt_in, false);
}

// All analytics work happens here: one detached thread, fully walled off from
// the rest of the process. The body is wrapped in catch_unwind as belt-and-
// suspenders — Rust already isolates spawned-thread panics from the parent,
// but a caught panic avoids abort-on-panic builds ever propagating.
#[cfg(target_os = "macos")]
fn spawn_heartbeat(storage: Arc<ClipboardStorage>, detailed_opt_in: bool, throttled: bool) {
    std::thread::Builder::new()
        .name("pasta-analytics-heartbeat".into())
        .spawn(move || {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                heartbeat_body(storage, detailed_opt_in, throttled);
            }));
        })
        .ok();
}

#[cfg(target_os = "macos")]
fn heartbeat_body(storage: Arc<ClipboardStorage>, detailed_opt_in: bool, throttled: bool) {
    if throttled {
        let state = load_analytics_state();
        if let Some(last) = state.last_sent_epoch
            && unix_now().saturating_sub(last) < HEARTBEAT_INTERVAL_SECONDS
        {
            return;
        }
    }
    let Some(install_id) = install_fingerprint() else {
        eprintln!("warning: analytics heartbeat skipped (no platform serial available)");
        return;
    };
    let clipboard_count = storage.total_item_count();
    let event = AnalyticsEvent {
        install_id: &install_id,
        event: "heartbeat",
        app_version: env!("CARGO_PKG_VERSION"),
        clipboard_count,
        os: detailed_opt_in.then_some("macos"),
        os_version: detailed_opt_in.then(macos_product_version),
        timestamp: detailed_opt_in.then(unix_now),
    };
    let Ok(payload) = serde_json::to_string(&event) else {
        return;
    };
    post_event_via_curl(&payload);
    save_analytics_state(&AnalyticsState {
        last_sent_epoch: Some(unix_now()),
    });
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_format_is_sha256_prefixed_hex() {
        let mut hasher = Sha256::new();
        hasher.update(FINGERPRINT_SALT);
        hasher.update(b":");
        hasher.update(b"EXAMPLESERIAL");
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let fingerprint = format!("sha256:{hex}");
        assert!(fingerprint.starts_with("sha256:"));
        assert_eq!(fingerprint.len(), "sha256:".len() + 64);
    }

    #[test]
    fn baseline_payload_carries_only_mandatory_fields() {
        let event = AnalyticsEvent {
            install_id: "sha256:abc",
            event: "heartbeat",
            app_version: "0.1.0",
            clipboard_count: 5,
            os: None,
            os_version: None,
            timestamp: None,
        };
        let json = serde_json::to_string(&event).expect("serialize baseline event");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse baseline json");
        let obj = value.as_object().expect("baseline json is an object");
        assert!(obj.contains_key("install_id"));
        assert!(obj.contains_key("app_version"));
        assert!(obj.contains_key("clipboard_count"));
        assert!(obj.contains_key("event"));
        assert!(!obj.contains_key("os"));
        assert!(!obj.contains_key("os_version"));
        assert!(!obj.contains_key("timestamp"));
    }

    #[test]
    fn detailed_payload_includes_opt_in_fields() {
        let event = AnalyticsEvent {
            install_id: "sha256:abc",
            event: "heartbeat",
            app_version: "0.1.0",
            clipboard_count: 5,
            os: Some("macos"),
            os_version: Some("14.4.1".to_owned()),
            timestamp: Some(1_700_000_000),
        };
        let json = serde_json::to_string(&event).expect("serialize detailed event");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse detailed json");
        let obj = value.as_object().expect("detailed json is an object");
        assert_eq!(obj.get("os").and_then(|v| v.as_str()), Some("macos"));
        assert_eq!(
            obj.get("os_version").and_then(|v| v.as_str()),
            Some("14.4.1")
        );
        assert_eq!(
            obj.get("timestamp").and_then(|v| v.as_u64()),
            Some(1_700_000_000)
        );
    }
}
