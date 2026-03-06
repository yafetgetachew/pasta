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
        let text = raw_text.trim();
        if text.is_empty() {
            return Ok(false);
        }

        let (item_type, tags) = classify_clipboard_text(text);
        let content_hash = content_hash(text);

        let mut conn = self.open()?;
        let tx = conn.transaction()?;

        let latest_hash: Option<String> = tx
            .query_row(
                "SELECT content_hash FROM clipboard_items ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if latest_hash.as_deref() == Some(content_hash.as_str()) {
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

            if normalized.is_empty() || record_matches_query(&record, &normalized) {
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
    let mut tags = Vec::new();

    let item_type = if looks_like_password(text) {
        tags.push("sensitive".to_owned());
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
    (item_type, tags)
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

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn record_matches_query(record: &ClipboardRecord, query: &str) -> bool {
    if record.content.to_lowercase().contains(query) {
        return true;
    }

    if record.item_type.as_str().contains(query) {
        return true;
    }

    record
        .tags
        .iter()
        .any(|tag| tag.to_lowercase().contains(query))
}
