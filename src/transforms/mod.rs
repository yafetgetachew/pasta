#[cfg(target_os = "macos")]
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
#[cfg(target_os = "macos")]
use chrono::{DateTime, Utc};

#[cfg(target_os = "macos")]
pub(crate) fn shell_quote_escape(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
pub(crate) fn json_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    let encoded =
        serde_json::to_string(input).map_err(|err| format!("json encode error: {err}"))?;
    Ok((encoded, "JSON-escaped to clipboard."))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn has_json_escape_markers(input: &str) -> bool {
    input.contains("\\n")
        || input.contains("\\t")
        || input.contains("\\\"")
        || input.contains("\\\\")
        || input.contains("\\u")
}

#[cfg(target_os = "macos")]
pub(crate) fn url_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    Ok((url_percent_encode(input), "URL-encoded to clipboard."))
}

#[cfg(target_os = "macos")]
pub(crate) fn url_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let decoded = url_percent_decode(input)?;
    Ok((decoded, "URL-decoded to clipboard."))
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some((value - b'a') + 10),
        b'A'..=b'F' => Some((value - b'A') + 10),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn base64_encode_transform(input: &str) -> Result<(String, &'static str), String> {
    Ok((
        BASE64_STANDARD.encode(input.as_bytes()),
        "Base64-encoded to clipboard.",
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn base64_decode_transform(input: &str) -> Result<(String, &'static str), String> {
    let compact: String = input.chars().filter(|ch| !ch.is_whitespace()).collect();
    let decoded_bytes = BASE64_STANDARD
        .decode(compact.as_bytes())
        .map_err(|err| format!("base64 decode error: {err}"))?;
    let decoded = String::from_utf8(decoded_bytes)
        .map_err(|_| "decoded base64 is binary (non UTF-8)".to_owned())?;
    Ok((decoded, "Base64-decoded to clipboard."))
}

#[cfg(target_os = "macos")]
fn looks_like_base64(value: &str) -> bool {
    if value.len() < 8 || value.len() % 4 != 0 {
        return false;
    }

    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-' | b'_')
    })
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn extract_first_pem_certificate(input: &str) -> Option<String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";

    let start = input.find(BEGIN)?;
    let rest = &input[start..];
    let end_offset = rest.find(END)?;
    let end = start + end_offset + END.len();
    Some(input[start..end].to_owned())
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn dn_attr_value(dn: &str, key: &str) -> Option<String> {
    for part in dn.split(',') {
        let (name, value) = part.split_once('=')?;
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim().to_owned());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn parse_openssl_datetime(value: &str) -> Option<DateTime<Utc>> {
    if value.trim().is_empty() {
        return None;
    }

    DateTime::parse_from_str(value.trim(), "%b %e %H:%M:%S %Y %Z")
        .ok()
        .map(|datetime| datetime.with_timezone(&Utc))
}
