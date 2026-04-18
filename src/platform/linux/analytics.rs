#[cfg(target_os = "linux")]
use crate::*;
#[cfg(target_os = "linux")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Outbound HTTP budget: hard ceiling for connect + TLS + write + read.
// Enforced by ureq's global timeout so a hung endpoint cannot keep the
// heartbeat thread alive past this deadline.
#[cfg(target_os = "linux")]
const HTTP_MAX_DURATION: Duration = Duration::from_secs(10);

#[cfg(target_os = "linux")]
const HEARTBEAT_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

// Build-time env vars (same contract as macOS — see platform/macos/analytics.rs
// for the full rationale around key rotation, anti-abuse and offline builds).
// Missing or empty required vars produce a no-op binary.
#[cfg(target_os = "linux")]
const ANALYTICS_API_KEY: Option<&str> = option_env!("PASTA_ANALYTICS_API_KEY");

#[cfg(target_os = "linux")]
const ANALYTICS_ENDPOINT: Option<&str> = option_env!("PASTA_ANALYTICS_ENDPOINT");

#[cfg(target_os = "linux")]
const ANALYTICS_SALT_OVERRIDE: Option<&str> = option_env!("PASTA_ANALYTICS_SALT");

#[cfg(target_os = "linux")]
const DEFAULT_FINGERPRINT_SALT: &[u8] = b"pasta-launcher/v1";

#[cfg(target_os = "linux")]
struct AnalyticsConfig {
    api_key: &'static str,
    endpoint: &'static str,
    salt: &'static [u8],
}

#[cfg(target_os = "linux")]
fn analytics_config() -> Option<AnalyticsConfig> {
    let api_key = ANALYTICS_API_KEY.filter(|k| !k.is_empty())?;
    let endpoint = ANALYTICS_ENDPOINT.filter(|e| !e.is_empty())?;
    let salt = ANALYTICS_SALT_OVERRIDE
        .filter(|s| !s.is_empty())
        .map(str::as_bytes)
        .unwrap_or(DEFAULT_FINGERPRINT_SALT);
    Some(AnalyticsConfig {
        api_key,
        endpoint,
        salt,
    })
}

#[cfg(target_os = "linux")]
#[derive(Debug, Serialize, Deserialize, Default)]
struct AnalyticsState {
    last_sent_epoch: Option<u64>,
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn analytics_state_path() -> Option<PathBuf> {
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::home_dir)?;
    let directory = base.join("PastaClipboard");
    fs::create_dir_all(&directory).ok()?;
    Some(directory.join("analytics-state.json"))
}

#[cfg(target_os = "linux")]
fn load_analytics_state() -> AnalyticsState {
    let Some(path) = analytics_state_path() else {
        return AnalyticsState::default();
    };
    let Ok(data) = fs::read_to_string(&path) else {
        return AnalyticsState::default();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn save_analytics_state(state: &AnalyticsState) {
    let Some(path) = analytics_state_path() else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&path, serialized);
    }
}

// /etc/machine-id is written once at install time by systemd-machine-id-setup
// (or equivalent on non-systemd distros) and is the canonical per-install
// identifier on Linux. It's a 32-char lowercase hex string — no PII on its own,
// but we still pepper it with the salt and SHA-256 the result so rotating the
// salt cycles the identifier space without coordination with the server. The
// dbus location is the historical fallback for minimal containers and some
// embedded systems; either present means we have an ID.
#[cfg(target_os = "linux")]
fn linux_machine_id() -> Option<String> {
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(raw) = fs::read_to_string(path) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn install_fingerprint(salt: &[u8]) -> Option<String> {
    let machine_id = linux_machine_id()?;
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(b":");
    hasher.update(machine_id.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Some(format!("sha256:{hex}"))
}

// Parses /etc/os-release per the freedesktop spec: KEY=VALUE per line, values
// may be double-quoted. We only care about PRETTY_NAME (distro-friendly label)
// with a fallback to VERSION_ID if PRETTY_NAME is missing.
#[cfg(target_os = "linux")]
fn linux_product_version() -> String {
    let Ok(raw) = fs::read_to_string("/etc/os-release") else {
        return "unknown".to_owned();
    };
    let mut pretty: Option<String> = None;
    let mut version_id: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "PRETTY_NAME" if !value.is_empty() => pretty = Some(value.to_owned()),
            "VERSION_ID" if !value.is_empty() => version_id = Some(value.to_owned()),
            _ => {}
        }
    }
    pretty
        .or(version_id)
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(target_os = "linux")]
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn post_event(event: &AnalyticsEvent<'_>, endpoint: &str, api_key: &str) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_MAX_DURATION))
        .build()
        .into();
    let _ = agent
        .post(endpoint)
        .header("Authorization", &format!("Bearer {api_key}"))
        .send_json(event);
}

#[cfg(target_os = "linux")]
pub(crate) fn maybe_send_heartbeat(storage: Arc<ClipboardStorage>, detailed_opt_in: bool) {
    if analytics_config().is_none() {
        return;
    }
    spawn_heartbeat(storage, detailed_opt_in, true);
}

#[cfg(target_os = "linux")]
pub(crate) fn send_heartbeat_now(storage: Arc<ClipboardStorage>, detailed_opt_in: bool) {
    if analytics_config().is_none() {
        return;
    }
    spawn_heartbeat(storage, detailed_opt_in, false);
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn heartbeat_body(storage: Arc<ClipboardStorage>, detailed_opt_in: bool, throttled: bool) {
    let Some(config) = analytics_config() else {
        return;
    };
    if throttled {
        let state = load_analytics_state();
        if let Some(last) = state.last_sent_epoch
            && unix_now().saturating_sub(last) < HEARTBEAT_INTERVAL_SECONDS
        {
            return;
        }
    }
    let Some(install_id) = install_fingerprint(config.salt) else {
        eprintln!("warning: analytics heartbeat skipped (no /etc/machine-id available)");
        return;
    };
    let clipboard_count = storage.total_item_count();
    let event = AnalyticsEvent {
        install_id: &install_id,
        event: "heartbeat",
        app_version: env!("CARGO_PKG_VERSION"),
        clipboard_count,
        os: detailed_opt_in.then_some("linux"),
        os_version: detailed_opt_in.then(linux_product_version),
        timestamp: detailed_opt_in.then(unix_now),
    };
    post_event(&event, config.endpoint, config.api_key);
    save_analytics_state(&AnalyticsState {
        last_sent_epoch: Some(unix_now()),
    });
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_format_is_sha256_prefixed_hex() {
        let mut hasher = Sha256::new();
        hasher.update(DEFAULT_FINGERPRINT_SALT);
        hasher.update(b":");
        hasher.update(b"abcdef1234567890abcdef1234567890");
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let fingerprint = format!("sha256:{hex}");
        assert!(fingerprint.starts_with("sha256:"));
        assert_eq!(fingerprint.len(), "sha256:".len() + 64);
    }

    #[test]
    fn fingerprint_salt_override_changes_output() {
        let default = {
            let mut h = Sha256::new();
            h.update(DEFAULT_FINGERPRINT_SALT);
            h.update(b":");
            h.update(b"MID");
            h.finalize()
        };
        let rotated = {
            let mut h = Sha256::new();
            h.update(b"pasta-launcher/v2");
            h.update(b":");
            h.update(b"MID");
            h.finalize()
        };
        assert_ne!(default.as_slice(), rotated.as_slice());
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
            os: Some("linux"),
            os_version: Some("Ubuntu 24.04 LTS".to_owned()),
            timestamp: Some(1_700_000_000),
        };
        let json = serde_json::to_string(&event).expect("serialize detailed event");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse detailed json");
        let obj = value.as_object().expect("detailed json is an object");
        assert_eq!(obj.get("os").and_then(|v| v.as_str()), Some("linux"));
        assert_eq!(
            obj.get("os_version").and_then(|v| v.as_str()),
            Some("Ubuntu 24.04 LTS")
        );
        assert_eq!(
            obj.get("timestamp").and_then(|v| v.as_u64()),
            Some(1_700_000_000)
        );
    }
}
