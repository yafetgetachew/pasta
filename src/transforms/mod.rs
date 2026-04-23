use base64::{
    Engine,
    engine::general_purpose::{
        STANDARD as BASE64_STANDARD, URL_SAFE as BASE64_URL_SAFE,
        URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD,
    },
};
use chrono::{DateTime, NaiveDateTime, Utc};
use qrcode::{EcLevel, QrCode};
use sha2::{Digest, Sha256};

// QR version 40 with EC level L holds 2953 bytes. Surface the number in the
// error so users know why oversize payloads are rejected.
const QR_MAX_BYTES: usize = 2953;

#[derive(Debug)]
pub(crate) struct QrMatrix {
    pub modules: Vec<bool>,
    pub width: usize,
}

pub(crate) fn shell_quote_escape(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

pub(crate) fn json_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    let encoded =
        serde_json::to_string(input).map_err(|err| format!("json encode error: {err}"))?;
    Ok((encoded, "JSON-escaped to clipboard."))
}

pub(crate) fn json_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let trimmed = input.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        let decoded = serde_json::from_str::<String>(trimmed)
            .map_err(|err| format!("json decode error: {err}"))?;
        return Ok((decoded, "JSON-unescaped to clipboard."));
    }

    let decoded = decode_json_escaped_string(trimmed)?;
    Ok((decoded, "JSON-unescaped to clipboard."))
}

fn decode_json_escaped_string(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("empty string".to_owned());
    }

    if !has_json_escape_markers(input) {
        return Err("input does not look JSON-escaped".to_owned());
    }

    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        return serde_json::from_str::<String>(input).map_err(|err| err.to_string());
    }

    let wrapped = format!("\"{input}\"");
    serde_json::from_str::<String>(&wrapped).map_err(|err| err.to_string())
}

fn has_json_escape_markers(input: &str) -> bool {
    input.contains("\\n")
        || input.contains("\\t")
        || input.contains("\\\"")
        || input.contains("\\\\")
        || input.contains("\\u")
}

pub(crate) fn url_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    Ok((url_percent_encode(input), "URL-encoded to clipboard."))
}

pub(crate) fn url_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let decoded = url_percent_decode(input)?;
    Ok((decoded, "URL-decoded to clipboard."))
}

fn url_percent_encode(input: &str) -> String {
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push_str(&format!("{byte:02X}"));
        }
    }
    output
}

fn url_percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0_usize;
    let decode_plus_as_space = input.contains('=') || input.contains('&') || input.contains('?');

    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err("malformed % escape".to_owned());
                }
                let Some(high) = hex_nibble(bytes[index + 1]) else {
                    return Err("invalid hex escape".to_owned());
                };
                let Some(low) = hex_nibble(bytes[index + 2]) else {
                    return Err("invalid hex escape".to_owned());
                };
                output.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                if decode_plus_as_space {
                    output.push(b' ');
                } else {
                    output.push(b'+');
                }
                index += 1;
            }
            value => {
                output.push(value);
                index += 1;
            }
        }
    }

    String::from_utf8(output).map_err(|_| "decoded URL is not utf-8".to_owned())
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some((value - b'a') + 10),
        b'A'..=b'F' => Some((value - b'A') + 10),
        _ => None,
    }
}

pub(crate) fn base64_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    Ok((
        BASE64_STANDARD.encode(input.as_bytes()),
        "Base64-encoded to clipboard.",
    ))
}

pub(crate) fn base64_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let compact: String = input.chars().filter(|ch| !ch.is_whitespace()).collect();

    // Try standard base64 first (alphabet: A-Z, a-z, 0-9, +, /, =).
    if let Ok(decoded_bytes) = BASE64_STANDARD.decode(compact.as_bytes()) {
        let decoded = String::from_utf8(decoded_bytes)
            .map_err(|_| "decoded base64 is binary (non UTF-8)".to_owned())?;
        return Ok((decoded, "Base64-decoded to clipboard."));
    }

    // If the input contains URL-safe characters (- or _), try URL-safe engines.
    if compact.contains('-') || compact.contains('_') {
        if let Ok(decoded_bytes) = BASE64_URL_SAFE.decode(compact.as_bytes()) {
            let decoded = String::from_utf8(decoded_bytes)
                .map_err(|_| "decoded base64 is binary (non UTF-8)".to_owned())?;
            return Ok((decoded, "Base64-decoded (URL-safe) to clipboard."));
        }
        if let Ok(decoded_bytes) = BASE64_URL_SAFE_NO_PAD.decode(compact.as_bytes()) {
            let decoded = String::from_utf8(decoded_bytes)
                .map_err(|_| "decoded base64 is binary (non UTF-8)".to_owned())?;
            return Ok((decoded, "Base64-decoded (URL-safe) to clipboard."));
        }
    }

    // All engines failed — return the standard error for diagnostics.
    let err = BASE64_STANDARD.decode(compact.as_bytes()).unwrap_err();
    Err(format!("base64 decode error: {err}"))
}

pub(crate) fn jwt_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() != 3 {
        return Err("not a JWT: expected 3 dot-separated segments".to_owned());
    }

    let decode_segment = |segment: &str| -> Result<serde_json::Value, String> {
        // JWT uses base64url without padding.
        let decoded_bytes = BASE64_URL_SAFE_NO_PAD
            .decode(segment.as_bytes())
            .or_else(|_| BASE64_URL_SAFE.decode(segment.as_bytes()))
            .map_err(|err| format!("base64 decode error: {err}"))?;
        serde_json::from_slice(&decoded_bytes).map_err(|err| format!("JSON parse error: {err}"))
    };

    let header = decode_segment(parts[0])?;
    let payload = decode_segment(parts[1])?;

    let header_pretty =
        serde_json::to_string_pretty(&header).unwrap_or_else(|_| header.to_string());
    let payload_pretty =
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());

    let mut summary = Vec::new();
    summary.push("JWT DECODED".to_owned());
    summary.push(String::new());

    // Extract useful claims.
    if let Some(alg) = header.get("alg").and_then(|v| v.as_str()) {
        summary.push(format!("Algorithm: {alg}"));
    }
    if let Some(typ) = header.get("typ").and_then(|v| v.as_str()) {
        summary.push(format!("Type: {typ}"));
    }
    if let Some(kid) = header.get("kid").and_then(|v| v.as_str()) {
        summary.push(format!("Key ID: {kid}"));
    }
    if let Some(sub) = payload.get("sub").and_then(|v| v.as_str()) {
        summary.push(format!("Subject: {sub}"));
    }
    if let Some(iss) = payload.get("iss").and_then(|v| v.as_str()) {
        summary.push(format!("Issuer: {iss}"));
    }
    if let Some(aud) = payload.get("aud") {
        let aud_str = match aud {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        };
        summary.push(format!("Audience: {aud_str}"));
    }
    if let Some(iat) = payload.get("iat").and_then(|v| v.as_i64()) {
        let dt = DateTime::from_timestamp(iat, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| iat.to_string());
        summary.push(format!("Issued At: {dt}"));
    }
    if let Some(exp) = payload.get("exp").and_then(|v| v.as_i64()) {
        let dt = DateTime::from_timestamp(exp, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| exp.to_string());
        let now = Utc::now().timestamp();
        let status = if exp < now {
            let ago = (now - exp) / 86400;
            format!(" (EXPIRED {} days ago)", ago)
        } else {
            let left = (exp - now) / 86400;
            format!(" ({left} days remaining)")
        };
        summary.push(format!("Expires: {dt}{status}"));
    }

    summary.push(String::new());
    summary.push("─── Header ───".to_owned());
    summary.push(header_pretty);
    summary.push(String::new());
    summary.push("─── Payload ───".to_owned());
    summary.push(payload_pretty);

    Ok((summary.join("\n"), "JWT decoded to clipboard."))
}

pub(crate) fn json_pretty_transform(input: &str) -> Result<(String, &'static str), String> {
    let value: serde_json::Value =
        serde_json::from_str(input.trim()).map_err(|err| format!("JSON parse error: {err}"))?;
    let pretty =
        serde_json::to_string_pretty(&value).map_err(|err| format!("JSON format error: {err}"))?;
    Ok((pretty, "JSON prettified to clipboard."))
}

pub(crate) fn json_minify_transform(input: &str) -> Result<(String, &'static str), String> {
    let value: serde_json::Value =
        serde_json::from_str(input.trim()).map_err(|err| format!("JSON parse error: {err}"))?;
    let compact =
        serde_json::to_string(&value).map_err(|err| format!("JSON format error: {err}"))?;
    Ok((compact, "JSON minified to clipboard."))
}

pub(crate) fn epoch_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("clipboard item is empty".to_owned());
    }

    // Try parsing as a unix timestamp (seconds or milliseconds).
    if let Ok(ts) = trimmed.parse::<i64>() {
        // Heuristic: if the number is > 1e12, treat as milliseconds.
        let (seconds, millis) = if ts > 1_000_000_000_000 {
            (ts / 1000, Some(ts % 1000))
        } else {
            (ts, None)
        };
        if let Some(dt) = DateTime::from_timestamp(seconds, 0) {
            let utc = dt.format("%Y-%m-%d %H:%M:%S UTC").to_string();
            let local_offset = *chrono::Local::now().offset();
            let local = dt
                .with_timezone(&local_offset)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string();
            let now = Utc::now().timestamp();
            let diff = seconds - now;
            let age = if diff < 0 {
                format_duration_ago(-diff)
            } else if diff > 0 {
                format_duration_from_now(diff)
            } else {
                "just now".to_owned()
            };
            let mut result = format!("UTC:   {utc}\nLocal: {local}\n({age})");
            if let Some(ms) = millis {
                result = format!("Epoch: {seconds}s + {ms}ms\n{result}");
            }
            return Ok((result, "Epoch decoded to clipboard."));
        }
    }

    // Try parsing as a float (fractional seconds).
    if let Ok(ts) = trimmed.parse::<f64>() {
        let seconds = ts as i64;
        let nanos = ((ts - seconds as f64) * 1_000_000_000.0) as u32;
        if let Some(dt) = DateTime::from_timestamp(seconds, nanos) {
            let utc = dt.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string();
            return Ok((utc, "Epoch decoded to clipboard."));
        }
    }

    // Try parsing a human-readable date string → epoch.
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ];
    for fmt in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            let epoch = dt.and_utc().timestamp();
            return Ok((epoch.to_string(), "Date converted to epoch in clipboard."));
        }
    }
    // Try the last format which is date-only.
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let epoch = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        return Ok((epoch.to_string(), "Date converted to epoch in clipboard."));
    }

    Err("not a recognized timestamp or date format".to_owned())
}

fn format_duration_ago(seconds: i64) -> String {
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    if seconds < 3600 {
        return format!("{}m ago", seconds / 60);
    }
    if seconds < 86400 {
        return format!("{}h {}m ago", seconds / 3600, (seconds % 3600) / 60);
    }
    let days = seconds / 86400;
    if days < 365 {
        return format!("{days}d ago");
    }
    format!("{}y {}d ago", days / 365, days % 365)
}

fn format_duration_from_now(seconds: i64) -> String {
    if seconds < 60 {
        return format!("in {seconds}s");
    }
    if seconds < 3600 {
        return format!("in {}m", seconds / 60);
    }
    if seconds < 86400 {
        return format!("in {}h {}m", seconds / 3600, (seconds % 3600) / 60);
    }
    let days = seconds / 86400;
    if days < 365 {
        return format!("in {days}d");
    }
    format!("in {}y {}d", days / 365, days % 365)
}

pub(crate) fn qr_encode_matrix(input: &str) -> Result<QrMatrix, String> {
    if input.is_empty() {
        return Err("clipboard item is empty".to_owned());
    }
    if input.len() > QR_MAX_BYTES {
        return Err(format!(
            "too big for QR code ({} bytes, max {QR_MAX_BYTES})",
            input.len()
        ));
    }

    let code = QrCode::with_error_correction_level(input.as_bytes(), EcLevel::L)
        .map_err(|_| format!("too big for QR code (max {QR_MAX_BYTES} bytes)"))?;

    let width = code.width();
    let modules = code
        .to_colors()
        .into_iter()
        .map(|color| matches!(color, qrcode::Color::Dark))
        .collect();

    Ok(QrMatrix { modules, width })
}

pub(crate) fn sha256_hash_transform(input: &str) -> Result<(String, &'static str), String> {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let hex: String = result.iter().map(|b| format!("{b:02x}")).collect();
    Ok((hex, "SHA256 hash copied to clipboard."))
}

pub(crate) fn content_stats_transform(input: &str) -> Result<(String, &'static str), String> {
    let lines = input.lines().count();
    let words = input.split_whitespace().count();
    let chars = input.chars().count();
    let bytes = input.len();

    let mut stats = vec![
        format!("Lines: {lines}"),
        format!("Words: {words}"),
        format!("Chars: {chars}"),
        format!("Bytes: {bytes}"),
    ];

    if bytes >= 1024 {
        let kb = bytes as f64 / 1024.0;
        if kb >= 1024.0 {
            stats.push(format!("Size:  {:.1} MB", kb / 1024.0));
        } else {
            stats.push(format!("Size:  {kb:.1} KB"));
        }
    }

    Ok((stats.join("\n"), "Content stats copied to clipboard."))
}

fn looks_like_base64(value: &str) -> bool {
    if value.len() < 8 || value.len() % 4 != 0 {
        return false;
    }

    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-' | b'_')
    })
}

pub(crate) fn public_cert_pem_info_transform(
    input: &str,
) -> Result<(String, &'static str), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("clipboard item is empty".to_owned());
    }

    if let Some(pem) = extract_first_pem_certificate(trimmed) {
        let raw = run_openssl_x509_details(pem.as_bytes(), false)?;
        return Ok((
            summarize_certificate_details(&raw),
            "Certificate info copied to clipboard.",
        ));
    }

    let compact: String = trimmed.chars().filter(|ch| !ch.is_whitespace()).collect();
    if looks_like_base64(&compact) {
        let der = BASE64_STANDARD
            .decode(compact.as_bytes())
            .map_err(|err| format!("certificate parse failed: {err}"))?;
        let raw = run_openssl_x509_details(&der, true)?;
        return Ok((
            summarize_certificate_details(&raw),
            "Certificate info copied to clipboard.",
        ));
    }

    Err("no PEM certificate found".to_owned())
}

fn extract_first_pem_certificate(input: &str) -> Option<String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";

    let start = input.find(BEGIN)?;
    let rest = &input[start..];
    let end_offset = rest.find(END)?;
    let end = start + end_offset + END.len();
    Some(input[start..end].to_owned())
}

fn run_openssl_x509_details(input: &[u8], der_input: bool) -> Result<String, String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut command = Command::new("openssl");
    command.arg("x509");
    if der_input {
        command.args(["-inform", "DER"]);
    }
    command.args([
        "-noout", "-subject", "-issuer", "-dates", "-serial", "-nameopt", "RFC2253",
    ]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| format!("openssl unavailable: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input)
            .map_err(|err| format!("failed writing cert to openssl: {err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("openssl failed: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            return Err("openssl could not parse certificate".to_owned());
        }
        return Err(stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn summarize_certificate_details(details: &str) -> String {
    let mut subject = String::new();
    let mut issuer = String::new();
    let mut serial = String::new();
    let mut not_before = String::new();
    let mut not_after = String::new();

    for line in details.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("subject=") {
            subject = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("issuer=") {
            issuer = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("serial=") {
            serial = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("notBefore=") {
            not_before = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("notAfter=") {
            not_after = value.trim().to_owned();
        }
    }

    let organization = dn_attr_value(&subject, "O").unwrap_or_else(|| "unknown".to_owned());
    let common_name = dn_attr_value(&subject, "CN").unwrap_or_else(|| "unknown".to_owned());
    let issuer_name = dn_attr_value(&issuer, "CN")
        .or_else(|| dn_attr_value(&issuer, "O"))
        .unwrap_or_else(|| "unknown".to_owned());

    let days_left = parse_openssl_datetime(&not_after)
        .map(|not_after_dt| (not_after_dt - Utc::now()).num_days())
        .map(|days| days.to_string())
        .unwrap_or_else(|| "unknown".to_owned());

    [
        "CERT INFO".to_owned(),
        format!("Org: {organization}"),
        format!("CN: {common_name}"),
        format!("Issuer: {issuer_name}"),
        format!(
            "Not Before: {}",
            if not_before.is_empty() {
                "unknown"
            } else {
                not_before.as_str()
            }
        ),
        format!(
            "Not After: {}",
            if not_after.is_empty() {
                "unknown"
            } else {
                not_after.as_str()
            }
        ),
        format!("Days Left: {days_left}"),
        format!(
            "Serial: {}",
            if serial.is_empty() {
                "unknown"
            } else {
                serial.as_str()
            }
        ),
    ]
    .join("\n")
}

fn dn_attr_value(dn: &str, key: &str) -> Option<String> {
    for part in dn.split(',') {
        let (name, value) = part.split_once('=')?;
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn parse_openssl_datetime(value: &str) -> Option<DateTime<Utc>> {
    if value.trim().is_empty() {
        return None;
    }

    DateTime::parse_from_str(value.trim(), "%b %e %H:%M:%S %Y %Z")
        .ok()
        .map(|datetime| datetime.with_timezone(&Utc))
}
