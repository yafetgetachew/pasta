use std::{collections::HashSet, fs, path::PathBuf};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use keyring::{Entry, Error as KeyringError};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

const KEYCHAIN_SERVICE: &str = "com.pasta.launcher";
const KEYCHAIN_ACCOUNT: &str = "clipboard_encryption_key_v1";

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum ClipboardItemType {
    Text,
    Code,
    Command,
    Password,
}

impl ClipboardItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Code => "code",
            Self::Command => "command",
            Self::Password => "password",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Code => "CODE",
            Self::Command => "CMD",
            Self::Password => "PASS",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "code" => Self::Code,
            "command" => Self::Command,
            "password" => Self::Password,
            _ => Self::Text,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClipboardRecord {
    pub id: i64,
    pub item_type: ClipboardItemType,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Clone)]
pub struct ClipboardStorage {
    db_path: PathBuf,
    crypto: CryptoBox,
}

impl ClipboardStorage {
    pub fn bootstrap(app_dir_name: &str) -> Result<Self> {
        let data_dir = dirs::data_local_dir()
            .or_else(dirs::home_dir)
            .context("unable to determine data directory")?
            .join(app_dir_name);
        fs::create_dir_all(&data_dir).context("unable to create data directory")?;

        let db_path = data_dir.join("clipboard.db");
        let storage = Self {
            db_path,
            crypto: CryptoBox::load_or_create()?,
        };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn upsert_clipboard_item(&self, raw_text: &str) -> Result<bool> {
        self.upsert_clipboard_item_with_hint(raw_text, false)
    }

    pub fn upsert_clipboard_item_with_hint(
        &self,
        raw_text: &str,
        force_secret: bool,
    ) -> Result<bool> {
        let text = raw_text.trim();
        if text.is_empty() {
            return Ok(false);
        }

        let (item_type, tags) = if force_secret {
            classify_clipboard_text_with_hint(text, true)
        } else {
            classify_clipboard_text(text)
        };
        let content_hash = content_hash(text);

        let mut conn = self.open()?;
        let tx = conn.transaction()?;

        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, item_type FROM clipboard_items WHERE content_hash = ?1 ORDER BY id DESC LIMIT 1",
                params![content_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        if let Some((existing_id, existing_item_type)) = existing {
            if force_secret && existing_item_type != "password" {
                let forced_type = ClipboardItemType::Password;
                let forced_tags = serde_json::to_string(&tags)?;
                let created_at = Utc::now().to_rfc3339();
                let encrypted_content = self.crypto.encrypt(text)?;
                tx.execute(
                    "UPDATE clipboard_items
                     SET item_type = ?1, content = ?2, is_encrypted = 1, tags = ?3, created_at = ?4
                     WHERE id = ?5",
                    params![
                        forced_type.as_str(),
                        encrypted_content,
                        forced_tags,
                        created_at,
                        existing_id
                    ],
                )?;
                tx.commit()?;
                return Ok(true);
            }

            tx.rollback()?;
            return Ok(false);
        }

        let (stored_content, is_encrypted) = if item_type == ClipboardItemType::Password {
            (self.crypto.encrypt(text)?, 1_i64)
        } else {
            (text.to_owned(), 0_i64)
        };

        let tags_json = serde_json::to_string(&tags)?;
        let created_at = Utc::now().to_rfc3339();

        tx.execute(
            "INSERT INTO clipboard_items (item_type, content, is_encrypted, tags, content_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                item_type.as_str(),
                stored_content,
                is_encrypted,
                tags_json,
                content_hash,
                created_at,
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn search_items(&self, query: &str, limit: usize) -> Result<Vec<ClipboardRecord>> {
        let normalized = query.trim().to_lowercase();
        let tag_only = normalized.starts_with('/');
        let effective_query = if tag_only {
            normalized.trim_start_matches('/').trim().to_owned()
        } else {
            normalized
        };
        let conn = self.open()?;

        let mut stmt = conn.prepare(
            "SELECT id, item_type, content, is_encrypted, tags, created_at
             FROM clipboard_items
             ORDER BY id DESC
             LIMIT 400",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            let item_type = ClipboardItemType::from_str(row.get::<_, String>(1)?.as_str());
            let mut content: String = row.get(2)?;
            let is_encrypted: i64 = row.get(3)?;
            let tags_json: String = row.get(4)?;
            let created_at: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

            if is_encrypted == 1 {
                if let Ok(decrypted) = self.crypto.decrypt(&content) {
                    content = decrypted;
                } else {
                    continue;
                }
            }

            let record = ClipboardRecord {
                id: row.get(0)?,
                item_type,
                content,
                tags,
                created_at,
            };

            if effective_query.is_empty()
                || record_matches_query(&record, &effective_query, tag_only)
            {
                output.push(record);
                if output.len() >= limit {
                    break;
                }
            }
        }

        Ok(output)
    }

    pub fn delete_item(&self, id: i64) -> Result<bool> {
        let conn = self.open()?;
        let deleted = conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    pub fn mark_item_as_secret(&self, id: i64) -> Result<bool> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;

        let existing: Option<(String, i64, String, String)> = tx
            .query_row(
                "SELECT content, is_encrypted, item_type, tags FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let Some((stored_content, is_encrypted, existing_item_type, existing_tags_json)) = existing
        else {
            tx.rollback()?;
            return Ok(false);
        };

        let plaintext = if is_encrypted == 1 {
            self.crypto.decrypt(&stored_content)?
        } else {
            stored_content
        };

        let (forced_type, forced_tags) = classify_clipboard_text_with_hint(&plaintext, true);
        let existing_tags: Vec<String> =
            serde_json::from_str(&existing_tags_json).unwrap_or_default();

        let mut merged = forced_tags;
        let mut seen: HashSet<String> = merged.iter().map(|tag| tag.to_ascii_lowercase()).collect();

        for tag in existing_tags {
            let lower = tag.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "text" | "code" | "command" | "type:text" | "type:code" | "type:command"
            ) {
                continue;
            }
            if seen.insert(lower) {
                merged.push(tag);
            }
        }

        merged.sort_unstable_by_key(|tag| tag.to_ascii_lowercase());
        let merged_tags_json = serde_json::to_string(&merged)?;

        let should_update = existing_item_type != forced_type.as_str() || is_encrypted != 1;
        if !should_update && existing_tags_json == merged_tags_json {
            tx.rollback()?;
            return Ok(false);
        }

        let encrypted_content = self.crypto.encrypt(&plaintext)?;
        tx.execute(
            "UPDATE clipboard_items
             SET item_type = ?1, content = ?2, is_encrypted = 1, tags = ?3
             WHERE id = ?4",
            params![
                forced_type.as_str(),
                encrypted_content,
                merged_tags_json,
                id,
            ],
        )?;

        tx.commit()?;
        Ok(true)
    }

    pub fn add_custom_tags(&self, id: i64, raw_tags: &[String]) -> Result<bool> {
        if raw_tags.is_empty() {
            return Ok(false);
        }

        let conn = self.open()?;
        let existing_tags_json: Option<String> = conn
            .query_row(
                "SELECT tags FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(existing_tags_json) = existing_tags_json else {
            return Ok(false);
        };

        let mut tags: Vec<String> = serde_json::from_str(&existing_tags_json).unwrap_or_default();
        let mut existing: HashSet<String> =
            tags.iter().map(|tag| tag.to_ascii_lowercase()).collect();

        let mut changed = false;
        for raw in raw_tags {
            let Some(normalized) = normalize_custom_tag(raw) else {
                continue;
            };
            let key = normalized.to_ascii_lowercase();
            if existing.insert(key) {
                tags.push(normalized);
                changed = true;
            }
        }

        if !changed {
            return Ok(false);
        }

        tags.sort_unstable();
        tags.dedup();
        let tags_json = serde_json::to_string(&tags)?;
        conn.execute(
            "UPDATE clipboard_items SET tags = ?1 WHERE id = ?2",
            params![tags_json, id],
        )?;
        Ok(true)
    }

    pub fn remove_custom_tags(&self, id: i64, raw_tags: &[String]) -> Result<bool> {
        if raw_tags.is_empty() {
            return Ok(false);
        }

        let conn = self.open()?;
        let existing_tags_json: Option<String> = conn
            .query_row(
                "SELECT tags FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(existing_tags_json) = existing_tags_json else {
            return Ok(false);
        };

        let tags: Vec<String> = serde_json::from_str(&existing_tags_json).unwrap_or_default();
        let remove_keys: HashSet<String> = raw_tags
            .iter()
            .filter_map(|raw| normalize_custom_tag(raw))
            .map(|normalized| normalized.to_ascii_lowercase())
            .collect();
        if remove_keys.is_empty() {
            return Ok(false);
        }

        let mut changed = false;
        let filtered: Vec<String> = tags
            .into_iter()
            .filter(|tag| {
                let lower = tag.to_ascii_lowercase();
                let remove = remove_keys.contains(&lower);
                if remove {
                    changed = true;
                }
                !remove
            })
            .collect();
        if !changed {
            return Ok(false);
        }

        let tags_json = serde_json::to_string(&filtered)?;
        conn.execute(
            "UPDATE clipboard_items SET tags = ?1 WHERE id = ?2",
            params![tags_json, id],
        )?;
        Ok(true)
    }

    fn open(&self) -> Result<Connection> {
        Connection::open(&self.db_path).context("unable to open sqlite database")
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.open()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_type TEXT NOT NULL,
                content TEXT NOT NULL,
                is_encrypted INTEGER NOT NULL DEFAULT 0,
                tags TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);",
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct CryptoBox {
    key: [u8; 32],
}

impl CryptoBox {
    fn load_or_create() -> Result<Self> {
        let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)?;

        match entry.get_password() {
            Ok(encoded) => {
                let decoded = BASE64
                    .decode(encoded.as_bytes())
                    .context("invalid keychain key encoding")?;
                let key: [u8; 32] = decoded
                    .try_into()
                    .map_err(|_| anyhow!("invalid key size in keychain"))?;
                Ok(Self { key })
            }
            Err(KeyringError::NoEntry) => {
                let mut key = [0_u8; 32];
                OsRng.fill_bytes(&mut key);
                entry.set_password(&BASE64.encode(key))?;
                Ok(Self { key })
            }
            Err(err) => Err(err.into()),
        }
    }

    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).context("invalid encryption key")?;
        let mut nonce_bytes = [0_u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow!("unable to encrypt clipboard content"))?;

        let mut output = nonce_bytes.to_vec();
        output.extend(ciphertext);
        Ok(BASE64.encode(output))
    }

    fn decrypt(&self, encrypted: &str) -> Result<String> {
        let bytes = BASE64
            .decode(encrypted.as_bytes())
            .context("invalid encrypted payload encoding")?;
        if bytes.len() <= 12 {
            return Err(anyhow!("encrypted payload is too short"));
        }

        let (nonce_bytes, cipher_bytes) = bytes.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.key).context("invalid encryption key")?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), cipher_bytes)
            .map_err(|_| anyhow!("unable to decrypt clipboard content"))?;
        String::from_utf8(plaintext).context("decrypted payload is not utf-8")
    }
}

fn classify_clipboard_text(text: &str) -> (ClipboardItemType, Vec<String>) {
    classify_clipboard_text_with_hint(text, false)
}

fn classify_clipboard_text_with_hint(
    text: &str,
    force_secret: bool,
) -> (ClipboardItemType, Vec<String>) {
    let mut tags = Vec::new();

    let item_type = if looks_like_password(text) {
        tags.push("sensitive".to_owned());
        tags.push("secret".to_owned());
        tags.push("pass".to_owned());
        ClipboardItemType::Password
    } else if looks_like_high_entropy_secret(text) {
        tags.push("sensitive".to_owned());
        tags.push("secret".to_owned());
        tags.push("pass".to_owned());
        tags.push("high_entropy".to_owned());
        tags.push("token".to_owned());
        ClipboardItemType::Password
    } else if looks_like_command(text) {
        tags.push("shell".to_owned());
        ClipboardItemType::Command
    } else if looks_like_code(text) {
        tags.push("code".to_owned());
        ClipboardItemType::Code
    } else {
        ClipboardItemType::Text
    };

    tags.push(item_type.as_str().to_owned());

    let mut enriched: HashSet<String> = tags.into_iter().collect();
    enriched.insert(format!("type:{}", item_type.as_str()));
    if text.lines().count() > 1 {
        enriched.insert("multiline".to_owned());
    } else {
        enriched.insert("singleline".to_owned());
    }
    if text.chars().count() > 240 {
        enriched.insert("long".to_owned());
    }
    if looks_like_url(text) {
        enriched.insert("url".to_owned());
    }
    if looks_like_path(text) {
        enriched.insert("path".to_owned());
    }
    if has_env_reference(text) {
        enriched.insert("env".to_owned());
    }
    if let Some(language) = detect_language_tag(item_type, text) {
        enriched.insert(language.to_owned());
        enriched.insert(format!("lang:{language}"));
    }

    if force_secret {
        enriched.retain(|tag| {
            let lower = tag.to_ascii_lowercase();
            !matches!(lower.as_str(), "text" | "code" | "command") && !lower.starts_with("type:")
        });
        enriched.insert("password".to_owned());
        enriched.insert("type:password".to_owned());
        enriched.insert("sensitive".to_owned());
        enriched.insert("secret".to_owned());
        enriched.insert("pass".to_owned());
    }

    let mut ordered: Vec<String> = enriched.into_iter().collect();
    ordered.sort_unstable();
    (
        if force_secret {
            ClipboardItemType::Password
        } else {
            item_type
        },
        ordered,
    )
}

fn looks_like_command(text: &str) -> bool {
    if text.contains('\n') {
        return false;
    }

    let value = text.trim();
    value.starts_with("$ ")
        || value.starts_with("./")
        || value.starts_with("sudo ")
        || value.starts_with("git ")
        || value.starts_with("docker ")
        || value.starts_with("kubectl ")
        || value.contains(" && ")
        || value.contains(" | ")
        || value.contains(" --")
}

fn looks_like_code(text: &str) -> bool {
    if text.lines().count() >= 2 {
        let markers = [
            "fn ", "class ", "import ", "export ", "const ", "let ", "=>", "{", "};", "</",
            "SELECT ", "INSERT ", "UPDATE ", "#include", "package ",
        ];
        return markers.iter().any(|marker| text.contains(marker));
    }

    false
}

fn looks_like_password(text: &str) -> bool {
    if text.contains(char::is_whitespace) {
        return false;
    }

    let len = text.chars().count();
    if !(12..=128).contains(&len) {
        return false;
    }

    let has_lower = text.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = text.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let has_symbol = text.chars().any(|c| !c.is_ascii_alphanumeric());
    if !(has_lower && has_upper && has_digit && has_symbol) {
        return false;
    }

    let unique_chars: HashSet<char> = text.chars().collect();
    unique_chars.len() * 2 >= len
}

fn looks_like_high_entropy_secret(text: &str) -> bool {
    if text.contains(char::is_whitespace) {
        return false;
    }

    let value = text.trim();
    let len = value.chars().count();
    if !(20..=256).contains(&len) {
        return false;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return false;
    }

    let mut has_alpha = false;
    let mut has_digit = false;
    for ch in value.chars() {
        if ch.is_ascii_alphabetic() {
            has_alpha = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else if !matches!(ch, '-' | '_' | '+' | '=' | '.' | ':' | '/' | '~') {
            return false;
        }
    }
    if !(has_alpha && has_digit) {
        return false;
    }

    shannon_entropy(value.as_bytes()) >= 3.6
}

fn shannon_entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }

    let mut counts = [0_usize; 256];
    for byte in bytes {
        counts[*byte as usize] += 1;
    }

    let len = bytes.len() as f64;
    let mut entropy = 0.0_f64;
    for count in counts {
        if count == 0 {
            continue;
        }
        let p = count as f64 / len;
        entropy -= p * p.log2();
    }

    entropy
}

fn looks_like_url(text: &str) -> bool {
    let value = text.trim();
    value.starts_with("https://") || value.starts_with("http://")
}

fn looks_like_path(text: &str) -> bool {
    if text.contains('\n') {
        return false;
    }

    let value = text.trim();
    if value.is_empty() {
        return false;
    }

    value.starts_with("~/")
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || (value.contains('/') && !value.contains("://") && !value.starts_with("$ "))
}

fn has_env_reference(text: &str) -> bool {
    if text.contains("${") || text.trim_start().starts_with("export ") {
        return true;
    }

    text.split_whitespace().any(|token| {
        let mut chars = token.chars();
        if chars.next() != Some('$') {
            return false;
        }

        let mut has_name = false;
        for ch in chars {
            if !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_') {
                return false;
            }
            has_name = true;
        }

        has_name
    })
}

fn detect_language_tag(item_type: ClipboardItemType, text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();

    if item_type == ClipboardItemType::Command || looks_like_shell(trimmed) {
        return Some("bash");
    }
    if lower.contains("[package]") || lower.contains("cargo.toml") {
        return Some("toml");
    }
    if lower.contains("```")
        || lower
            .lines()
            .any(|line| line.trim_start().starts_with("# "))
    {
        return Some("markdown");
    }
    if looks_like_json(trimmed) {
        return Some("json");
    }
    if looks_like_yaml(trimmed) {
        return Some("yaml");
    }
    if lower.contains("<html") || lower.contains("</") || lower.contains("<div") {
        return Some("html");
    }
    if lower.contains('{')
        && lower.contains('}')
        && (lower.contains(':') || lower.contains(';'))
        && (lower.contains("color:") || lower.contains("display:") || lower.contains("margin:"))
    {
        return Some("css");
    }
    if contains_any(
        &lower,
        &[
            "select ",
            "insert into ",
            "update ",
            "delete from ",
            "where ",
        ],
    ) && lower.contains(" from ")
    {
        return Some("sql");
    }
    if contains_any(&lower, &["fn ", "impl ", "mut ", "let ", "::", "cargo "]) {
        return Some("rust");
    }
    if contains_any(
        &lower,
        &[
            "interface ",
            "type ",
            ": string",
            ": number",
            " as const",
            "readonly ",
            "import type ",
        ],
    ) {
        return Some("typescript");
    }
    if contains_any(
        &lower,
        &[
            "function ",
            "console.log",
            "=>",
            "module.exports",
            "require(",
        ],
    ) {
        return Some("javascript");
    }
    if contains_any(
        &lower,
        &["def ", "import ", "from ", "print(", "__name__", "lambda "],
    ) && trimmed.contains(':')
    {
        return Some("python");
    }
    if contains_any(&lower, &["package main", "func ", "fmt.", "go "]) {
        return Some("go");
    }
    if contains_any(
        &lower,
        &[
            "public class",
            "public static void main",
            "system.out.println",
        ],
    ) {
        return Some("java");
    }
    if contains_any(&lower, &["#include", "std::", "int main(", "cout <<"]) {
        return Some("cpp");
    }
    if looks_like_toml(trimmed) {
        return Some("toml");
    }
    if item_type == ClipboardItemType::Code {
        return Some("code");
    }

    None
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_shell(text: &str) -> bool {
    text.starts_with("#!/bin/bash")
        || text.starts_with("#!/usr/bin/env bash")
        || text.starts_with("#!/bin/zsh")
        || text.starts_with("#!/usr/bin/env zsh")
}

fn looks_like_json(text: &str) -> bool {
    if !(text.starts_with('{') || text.starts_with('[')) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

fn looks_like_yaml(text: &str) -> bool {
    let mut has_pairs = 0_usize;
    for line in text.lines().take(80) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            continue;
        }
        if trimmed.contains(':')
            && !trimmed.contains('{')
            && !trimmed.contains('}')
            && !trimmed.contains(';')
        {
            has_pairs += 1;
        }
    }

    has_pairs >= 2
}

fn looks_like_toml(text: &str) -> bool {
    let mut has_section = false;
    let mut has_assignment = false;

    for line in text.lines().take(80) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            has_section = true;
        }
        if trimmed.contains('=') {
            has_assignment = true;
        }
    }

    has_assignment && (has_section || text.lines().count() > 1)
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn record_matches_query(record: &ClipboardRecord, query: &str, tag_only: bool) -> bool {
    if tag_only {
        let tag_terms: Vec<&str> = query
            .split_whitespace()
            .map(|term| term.trim_start_matches('/').trim())
            .filter(|term| !term.is_empty())
            .collect();

        if tag_terms.is_empty() {
            return true;
        }

        return tag_terms
            .into_iter()
            .all(|term| record_matches_tag(record, term));
    }

    let content = record.content.to_lowercase();
    if content.contains(query) {
        return true;
    }

    if record_matches_tag(record, query) {
        return true;
    }

    let query_terms = tokenize_search_terms(query);
    if query_terms.is_empty() {
        return false;
    }

    let content_terms = tokenize_search_terms(&content);
    query_terms
        .iter()
        .all(|term| term_matches_record(record, term, &content, &content_terms))
}

fn record_matches_tag(record: &ClipboardRecord, query: &str) -> bool {
    let normalized = query.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }

    if record.item_type == ClipboardItemType::Password && query_matches_secret_intent(&normalized) {
        return true;
    }

    searchable_tag_terms(record).into_iter().any(|candidate| {
        candidate.contains(&normalized)
            || (normalized.len() >= 3 && normalized.contains(&candidate) && candidate.len() >= 2)
            || fuzzy_token_match(&normalized, &candidate)
    })
}

fn query_matches_secret_intent(query: &str) -> bool {
    let normalized = query.trim();
    if normalized.len() < 3 {
        return false;
    }

    const SECRET_ALIASES: [&str; 8] = [
        "pass",
        "password",
        "secret",
        "token",
        "credential",
        "api_key",
        "apikey",
        "pwd",
    ];

    SECRET_ALIASES
        .iter()
        .any(|alias| alias.starts_with(normalized) || normalized.starts_with(alias))
}

fn searchable_tag_terms(record: &ClipboardRecord) -> HashSet<String> {
    let mut terms = HashSet::new();
    insert_item_type_aliases(record.item_type, &mut terms);

    for raw in &record.tags {
        insert_tag_variants(raw, &mut terms);
    }

    if let Some(language) = detect_language_tag(record.item_type, &record.content) {
        terms.insert(format!("lang:{language}"));
        insert_language_aliases(language, &mut terms);
    }

    if terms.contains("multiline") {
        terms.insert("multi".to_owned());
    }
    if terms.contains("singleline") {
        terms.insert("single".to_owned());
    }
    if terms.contains("sensitive") || terms.contains("secret") || terms.contains("pass") {
        terms.insert("secret".to_owned());
        terms.insert("pass".to_owned());
        terms.insert("password".to_owned());
    }
    if terms.contains("command") || terms.contains("shell") {
        terms.insert("cmd".to_owned());
    }
    if terms.contains("yaml") {
        terms.insert("yml".to_owned());
    }
    if terms.contains("markdown") {
        terms.insert("md".to_owned());
    }
    if terms.contains("typescript") {
        terms.insert("ts".to_owned());
    }
    if terms.contains("javascript") {
        terms.insert("js".to_owned());
    }
    if terms.contains("python") {
        terms.insert("py".to_owned());
    }
    if terms.contains("cpp") {
        terms.insert("c++".to_owned());
        terms.insert("cxx".to_owned());
    }

    terms
}

fn insert_tag_variants(raw: &str, terms: &mut HashSet<String>) {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return;
    }

    terms.insert(normalized.clone());
    for token in tokenize_search_terms(&normalized) {
        terms.insert(token.to_owned());
    }

    if let Some(stripped) = normalized.strip_prefix("type:")
        && !stripped.is_empty()
    {
        insert_item_type_aliases(ClipboardItemType::from_str(stripped), terms);
    }
    if let Some(stripped) = normalized.strip_prefix("lang:")
        && !stripped.is_empty()
    {
        insert_language_aliases(stripped, terms);
    }
}

fn insert_item_type_aliases(item_type: ClipboardItemType, terms: &mut HashSet<String>) {
    terms.insert(item_type.as_str().to_owned());
    terms.insert(item_type.label().to_ascii_lowercase());

    match item_type {
        ClipboardItemType::Text => {
            terms.insert("plain".to_owned());
        }
        ClipboardItemType::Code => {}
        ClipboardItemType::Command => {
            terms.insert("shell".to_owned());
            terms.insert("terminal".to_owned());
        }
        ClipboardItemType::Password => {
            terms.insert("secret".to_owned());
        }
    }
}

fn insert_language_aliases(language: &str, terms: &mut HashSet<String>) {
    let lower = language.to_ascii_lowercase();
    terms.insert(lower.clone());

    match lower.as_str() {
        "bash" => {
            terms.insert("sh".to_owned());
            terms.insert("zsh".to_owned());
            terms.insert("shell".to_owned());
        }
        "rust" => {
            terms.insert("rs".to_owned());
        }
        "python" => {
            terms.insert("py".to_owned());
        }
        "typescript" => {
            terms.insert("ts".to_owned());
            terms.insert("tsx".to_owned());
        }
        "javascript" => {
            terms.insert("js".to_owned());
            terms.insert("node".to_owned());
            terms.insert("nodejs".to_owned());
        }
        "go" => {
            terms.insert("golang".to_owned());
        }
        "cpp" => {
            terms.insert("c++".to_owned());
            terms.insert("cxx".to_owned());
        }
        "yaml" => {
            terms.insert("yml".to_owned());
        }
        "markdown" => {
            terms.insert("md".to_owned());
        }
        _ => {}
    }
}

fn normalize_custom_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized: String = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/'))
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    if normalized.is_empty() {
        return None;
    }

    Some(normalized)
}

fn term_matches_record(
    record: &ClipboardRecord,
    query_term: &str,
    content: &str,
    content_terms: &[&str],
) -> bool {
    if content.contains(query_term) || record_matches_tag(record, query_term) {
        return true;
    }

    content_terms
        .iter()
        .any(|term| fuzzy_token_match(query_term, term))
}

fn tokenize_search_terms(value: &str) -> Vec<&str> {
    value
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/')))
        .filter(|term| !term.is_empty())
        .collect()
}

fn fuzzy_token_match(query_term: &str, candidate: &str) -> bool {
    if candidate.contains(query_term) || query_term.contains(candidate) {
        return true;
    }

    if query_term.len() < 4 || candidate.len() < 4 {
        return false;
    }

    let max_distance = if query_term.len() <= 6 { 1 } else { 2 };
    levenshtein_with_limit(query_term, candidate, max_distance)
}

fn levenshtein_with_limit(a: &str, b: &str, limit: usize) -> bool {
    if a == b {
        return true;
    }

    let (short, long) = if a.len() <= b.len() {
        (a.as_bytes(), b.as_bytes())
    } else {
        (b.as_bytes(), a.as_bytes())
    };

    if long.len().saturating_sub(short.len()) > limit {
        return false;
    }

    let mut prev: Vec<usize> = (0..=short.len()).collect();
    let mut curr = vec![0; short.len() + 1];

    for (i, &long_byte) in long.iter().enumerate() {
        curr[0] = i + 1;
        let mut row_min = curr[0];

        for (j, &short_byte) in short.iter().enumerate() {
            let cost = usize::from(long_byte != short_byte);
            let deletion = prev[j + 1] + 1;
            let insertion = curr[j] + 1;
            let substitution = prev[j] + cost;
            let best = deletion.min(insertion).min(substitution);
            curr[j + 1] = best;
            row_min = row_min.min(best);
        }

        if row_min > limit {
            return false;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[short.len()] <= limit
}
