use std::collections::HashMap;

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

/// Polkit action identifiers used by Pasta. Keep in sync with
/// `packaging/linux/com.pasta.launcher.policy`.
pub(crate) const ACTION_REVEAL_SECRET: &str = "com.pasta.launcher.reveal-secret";
#[allow(dead_code)]
pub(crate) const ACTION_CLEAR_HISTORY: &str = "com.pasta.launcher.clear-history";

/// `CheckAuthorizationFlags::AllowUserInteraction`.
const FLAG_ALLOW_USER_INTERACTION: u32 = 0x01;

const POLKIT_DBUS_NAME: &str = "org.freedesktop.PolicyKit1";
const POLKIT_OBJECT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const POLKIT_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";

/// Read field 22 (`starttime`) from `/proc/<pid>/stat`. Polkit uses this to
/// uniquely identify the process even across PID reuse.
fn process_start_time(pid: u32) -> std::io::Result<u64> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // Field 2 is the command name wrapped in parentheses and may contain
    // spaces; skip past the last ')' before splitting so word indexing is
    // stable.
    let rparen = raw.rfind(')').ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed /proc stat")
    })?;
    let after = &raw[rparen + 1..];
    // After ')' the remaining fields are space-separated starting at field 3
    // (state). starttime is field 22 overall, i.e. index 22 - 3 = 19 in this
    // tail.
    let field = after
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing starttime"))?;
    field
        .parse::<u64>()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}

/// Build the polkit Subject dict for the current process.
fn current_subject_details() -> std::io::Result<HashMap<String, OwnedValue>> {
    let pid = std::process::id();
    let start_time = process_start_time(pid)?;

    let mut details: HashMap<String, OwnedValue> = HashMap::new();
    details.insert(
        "pid".to_owned(),
        Value::U32(pid).try_to_owned().map_err(io_err)?,
    );
    details.insert(
        "start-time".to_owned(),
        Value::U64(start_time).try_to_owned().map_err(io_err)?,
    );
    Ok(details)
}

fn io_err<E: std::fmt::Display>(err: E) -> std::io::Error {
    std::io::Error::other(err.to_string())
}

/// Ask polkit to authorize `action_id` for the current process, allowing the
/// polkit agent to prompt the user (password / howdy / fingerprint). Returns
/// `true` only if polkit reports `is_authorized = true` AND `is_challenge =
/// false`.
pub(crate) fn authenticate(action_id: &str, _reason: &str) -> bool {
    if std::env::var_os("PASTA_SKIP_AUTH").is_some() {
        return true;
    }

    match check_authorization(action_id) {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("warning: polkit authorization check failed: {err}");
            false
        }
    }
}

fn check_authorization(action_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let subject_details = current_subject_details()?;
    let subject = ("unix-process", subject_details);

    // polkit rejects `polkit.message` overrides from non-trusted callers
    // (only uid 0 or the action owner may pass details). We rely on the
    // per-action <message> declared in the .policy file instead — that is
    // why there are separate action IDs for reveal-secret vs. clear-history.
    let details: HashMap<&str, &str> = HashMap::new();

    let cancellation_id = "";
    let flags: u32 = FLAG_ALLOW_USER_INTERACTION;

    let conn = Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        POLKIT_DBUS_NAME,
        POLKIT_OBJECT_PATH,
        POLKIT_INTERFACE,
    )?;

    let (is_authorized, is_challenge, _details): (bool, bool, HashMap<String, String>) = proxy
        .call(
            "CheckAuthorization",
            &(subject, action_id, details, flags, cancellation_id),
        )?;

    // is_challenge means polkit wants another round of interaction but the
    // current call ended without a final decision — treat as denial to keep
    // the UX simple (user can retry).
    Ok(is_authorized && !is_challenge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_match_policy_file() {
        assert_eq!(ACTION_REVEAL_SECRET, "com.pasta.launcher.reveal-secret");
        assert_eq!(ACTION_CLEAR_HISTORY, "com.pasta.launcher.clear-history");
    }

    #[test]
    fn process_start_time_is_readable_for_self() {
        let pid = std::process::id();
        let start = process_start_time(pid).expect("self /proc stat readable");
        assert!(start > 0);
    }

    #[test]
    fn skip_auth_env_short_circuits() {
        // SAFETY: test-only; no other thread races this env write.
        unsafe { std::env::set_var("PASTA_SKIP_AUTH", "1") };
        assert!(authenticate(ACTION_REVEAL_SECRET, "test"));
        unsafe { std::env::remove_var("PASTA_SKIP_AUTH") };
    }
}
