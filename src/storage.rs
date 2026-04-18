use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use keyring::{Entry, Error as KeyringError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::neural_embed::NeuralEmbedder;

const KEYCHAIN_SERVICE: &str = "com.pasta.launcher";
const KEYCHAIN_ACCOUNT: &str = "clipboard_encryption_key_v1";
const SEMANTIC_VECTOR_DIM: usize = 192;
const SEMANTIC_MIN_MATCH_SCORE: f32 = 0.22;
pub(crate) const SEMANTIC_MIN_QUERY_CHARS: usize = 3;
const SEMANTIC_EMBEDDING_CACHE_MAX: usize = 12_000;
pub(crate) const SEMANTIC_SOURCE_TEXT_LIMIT: usize = 4_096;
const SEARCH_FUZZY_TEXT_LIMIT: usize = 4_096;
const TAG_ONLY_PREFIX: char = ':';
const BOWL_TAG_PREFIX: &str = "bowl:";
const SEARCH_COMMAND_BOWL: &str = "b";
const SEARCH_COMMAND_EXPORT: &str = "e";

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardParameter {
    pub name: String,
    pub target: String,
}

#[derive(Clone, Debug)]
pub struct ClipboardRecord {
    pub id: i64,
    pub item_type: ClipboardItemType,
    pub content: String,
    pub description: String,
    pub tags: Vec<String>,
    pub parameters: Vec<ClipboardParameter>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchQuery {
    Default { effective_query: String },
    TagOnly { effective_query: String },
    Bowl { bowl_query: String },
    ExportBowl { bowl_query: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SearchExecution {
    Fast,
    Semantic,
    Neural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretClassificationMode {
    Auto,
    ForceSecret,
    Ignore,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BowlExportBundle {
    pub(crate) kind: String,
    pub(crate) version: u8,
    pub(crate) bowl: String,
    pub(crate) exported_at: String,
    pub(crate) items: Vec<BowlExportItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BowlExportItem {
    pub(crate) item_type: String,
    pub(crate) content: String,
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) parameters: Vec<BowlExportParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hash_embedding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) neural_embedding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) embedding_model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BowlExportParameter {
    pub(crate) name: String,
    #[serde(default, alias = "target")]
    pub(crate) default_value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BowlImportSummary {
    pub(crate) bowl: String,
    pub(crate) imported_count: usize,
}

#[derive(Debug)]
struct ScoredRecord {
    record: ClipboardRecord,
    semantic_score: f32,
    neural_score: f32,
    lexical_score: f32,
}

#[derive(Clone)]
struct IndexedRecord {
    record: ClipboardRecord,
    content_hash: String,
    content_lower: String,
    description_lower: String,
    search_tokens: Vec<String>,
}

#[derive(Default)]
struct MemorySearchIndex {
    order_desc_ids: Vec<i64>,
    by_id: HashMap<i64, IndexedRecord>,
}

#[derive(Clone)]
pub struct ClipboardStorage {
    db_path: PathBuf,
    crypto: CryptoBox,
    semantic_embedding_cache: Arc<Mutex<HashMap<String, Vec<f32>>>>,
    neural_embedding_cache: Arc<Mutex<HashMap<String, Vec<f32>>>>,
    neural_embedder: Arc<Mutex<Option<Arc<NeuralEmbedder>>>>,
    memory_index: Arc<Mutex<MemorySearchIndex>>,
}

impl ClipboardStorage {
    pub fn bootstrap(app_dir_name: &str) -> Result<Self> {
        let db_path = primary_db_path(app_dir_name)?;
        let crypto = match CryptoBox::load_or_create() {
            Ok(crypto) => crypto,
            Err(err) => {
                eprintln!(
                    "warning: secure storage unavailable; using ephemeral encryption for this session: {err}"
                );
                CryptoBox::ephemeral()
            }
        };
        let storage = Self {
            db_path,
            crypto,
            semantic_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedder: Arc::new(Mutex::new(None)),
            memory_index: Arc::new(Mutex::new(MemorySearchIndex::default())),
        };
        storage.apply_persistent_pragmas()?;
        storage.init_schema()?;
        storage.rebuild_memory_index()?;
        Ok(storage)
    }

    pub fn bootstrap_fallback(app_dir_name: &str) -> Result<Self> {
        let db_path = fallback_db_path(app_dir_name)?;

        // Previous fallback sessions leave behind db files encrypted with an ephemeral
        // key that can never be decrypted again; sweep them so they do not accumulate.
        if let Some(data_dir) = db_path.parent() {
            sweep_orphaned_fallback_dbs(data_dir, std::process::id());
        }

        let storage = Self {
            db_path,
            crypto: CryptoBox::ephemeral(),
            semantic_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedder: Arc::new(Mutex::new(None)),
            memory_index: Arc::new(Mutex::new(MemorySearchIndex::default())),
        };
        storage.apply_persistent_pragmas()?;
        storage.init_schema()?;
        storage.rebuild_memory_index()?;
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
                self.sync_index_record_from_db(existing_id)?;
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
            "INSERT INTO clipboard_items (item_type, content, is_encrypted, tags, parameters, description, content_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item_type.as_str(),
                stored_content,
                is_encrypted,
                tags_json,
                "[]",
                "",
                content_hash,
                created_at,
            ],
        )?;
        let inserted_id = tx.last_insert_rowid();
        tx.commit()?;
        self.sync_index_record_from_db(inserted_id)?;
        Ok(true)
    }

    pub fn search_items(
        &self,
        query: &str,
        limit: usize,
        pasta_brain: bool,
        execution: SearchExecution,
        query_generation: u64,
        latest_query_generation: Option<&AtomicU64>,
    ) -> Result<Vec<ClipboardRecord>> {
        let is_canceled = || {
            latest_query_generation
                .is_some_and(|current| current.load(Ordering::Acquire) != query_generation)
        };
        if is_canceled() {
            return Ok(Vec::new());
        }
        let parsed_query = parse_search_query(query);
        let (effective_query, tag_only, bowl_only) = match &parsed_query {
            SearchQuery::Default { effective_query } => (effective_query.as_str(), false, false),
            SearchQuery::TagOnly { effective_query } => (effective_query.as_str(), true, false),
            SearchQuery::Bowl { bowl_query } | SearchQuery::ExportBowl { bowl_query } => {
                (bowl_query.as_str(), false, true)
            }
        };
        if bowl_only {
            return Ok(self.search_items_by_bowl(effective_query, limit));
        }

        let use_semantic_search = matches!(
            execution,
            SearchExecution::Semantic | SearchExecution::Neural
        ) && !tag_only
            && effective_query.chars().count() >= SEMANTIC_MIN_QUERY_CHARS
            && !effective_query.is_empty();
        let use_neural_search = matches!(execution, SearchExecution::Neural) && use_semantic_search;
        let query_terms = if effective_query.is_empty() {
            Vec::new()
        } else {
            semantic_tokenize(effective_query)
        };
        let query_embedding =
            use_semantic_search.then(|| semantic_embedding(effective_query, &query_terms));
        let neural_embedder = if pasta_brain && use_neural_search {
            self.neural_embedder
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
        } else {
            None
        };
        let query_neural_embedding = if use_neural_search {
            neural_embedder
                .as_ref()
                .map(|embedder| embedder.embed(effective_query, &query_terms))
        } else {
            None
        };
        let index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut output = Vec::new();
        let mut ranked = Vec::new();
        let mut lexical_hits = 0_usize;

        for id in &index.order_desc_ids {
            if is_canceled() {
                return Ok(Vec::new());
            }
            let Some(indexed) = index.by_id.get(id) else {
                continue;
            };
            let record = &indexed.record;

            if effective_query.is_empty() {
                output.push(record.clone());
                if output.len() >= limit {
                    break;
                }
                continue;
            }

            let lexical_match =
                record_matches_query_indexed(record, effective_query, tag_only, Some(indexed));
            if !use_semantic_search {
                if lexical_match {
                    output.push(record.clone());
                    if output.len() >= limit {
                        break;
                    }
                }
                continue;
            }

            if lexical_match {
                lexical_hits += 1;
                let lexical_score = lexical_match_score(record, effective_query, &query_terms);
                ranked.push(ScoredRecord {
                    record: record.clone(),
                    semantic_score: 0.0,
                    neural_score: 0.0,
                    lexical_score,
                });
                continue;
            }

            // Once we have enough lexical hits to fill the visible result set, semantic-only
            // candidates can no longer beat lexical-ranked rows.
            if lexical_hits >= limit {
                continue;
            }

            // Never surface secrets as semantic-only matches.
            if record.item_type == ClipboardItemType::Password {
                continue;
            }

            let Some(query_embedding) = query_embedding.as_ref() else {
                continue;
            };
            if is_canceled() {
                return Ok(Vec::new());
            }
            let fallback_cache_key;
            let cache_key = if indexed.content_hash.is_empty() {
                fallback_cache_key = format!("id:{}", record.id);
                fallback_cache_key.as_str()
            } else {
                indexed.content_hash.as_str()
            };
            let mut semantic_seed_terms = tags_without_bowl(&record.tags);
            if !record.description.is_empty() {
                semantic_seed_terms
                    .extend(semantic_tokenize(&record.description.to_ascii_lowercase()));
            }
            let record_embedding =
                self.cached_semantic_embedding(cache_key, &record.content, &semantic_seed_terms);
            let semantic_score = cosine_similarity(query_embedding, &record_embedding);

            let neural_score = if let Some(query_neural) = query_neural_embedding.as_ref() {
                if is_canceled() {
                    return Ok(Vec::new());
                }
                let record_neural = self.cached_neural_embedding(
                    cache_key,
                    &record.content,
                    &semantic_seed_terms,
                    neural_embedder.as_ref(),
                );
                cosine_similarity(query_neural, &record_neural)
            } else {
                0.0
            };

            let best_score = semantic_score.max(neural_score);
            if best_score >= SEMANTIC_MIN_MATCH_SCORE {
                ranked.push(ScoredRecord {
                    record: record.clone(),
                    semantic_score,
                    neural_score,
                    lexical_score: 0.0,
                });
            }
        }

        if use_semantic_search {
            ranked.sort_by(|left, right| {
                combined_search_score(right)
                    .total_cmp(&combined_search_score(left))
                    .then_with(|| right.record.id.cmp(&left.record.id))
            });

            output = ranked
                .into_iter()
                .take(limit)
                .map(|item| item.record)
                .collect();
        }

        Ok(output)
    }

    pub fn suggest_search_tokens(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        match parse_search_query(query) {
            SearchQuery::TagOnly { .. } => {
                let Some(context) = tag_search_autocomplete_context(query) else {
                    return Vec::new();
                };

                let index = self
                    .memory_index
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let mut candidate_counts: HashMap<String, usize> = HashMap::new();

                for id in &index.order_desc_ids {
                    let Some(indexed) = index.by_id.get(id) else {
                        continue;
                    };

                    for candidate in searchable_tag_terms(&indexed.record) {
                        if tag_search_suggestion_match_rank(&candidate, &context).is_some() {
                            *candidate_counts.entry(candidate).or_insert(0) += 1;
                        }
                    }
                }

                let mut suggestions: Vec<(String, u8, usize)> = candidate_counts
                    .into_iter()
                    .filter_map(|(candidate, count)| {
                        let rank = tag_search_suggestion_match_rank(&candidate, &context)?;
                        Some((candidate, rank, count))
                    })
                    .collect();

                suggestions.sort_by(|left, right| {
                    left.1
                        .cmp(&right.1)
                        .then_with(|| right.2.cmp(&left.2))
                        .then_with(|| left.0.len().cmp(&right.0.len()))
                        .then_with(|| left.0.cmp(&right.0))
                });

                suggestions
                    .into_iter()
                    .take(limit)
                    .map(|(candidate, _, _)| candidate)
                    .collect()
            }
            SearchQuery::Bowl { bowl_query } | SearchQuery::ExportBowl { bowl_query } => {
                self.suggest_bowl_names(&bowl_query, limit)
            }
            SearchQuery::Default { .. } => Vec::new(),
        }
    }

    fn search_items_by_bowl(&self, bowl_query: &str, limit: usize) -> Vec<ClipboardRecord> {
        let normalized_query = bowl_query.trim().to_ascii_lowercase();
        let index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut output = Vec::new();

        for id in &index.order_desc_ids {
            let Some(indexed) = index.by_id.get(id) else {
                continue;
            };
            if record_matches_bowl(&indexed.record, &normalized_query) {
                output.push(indexed.record.clone());
                if output.len() >= limit {
                    break;
                }
            }
        }

        output
    }

    pub fn suggest_bowl_names(&self, bowl_query: &str, limit: usize) -> Vec<String> {
        let normalized_query = bowl_query.trim().to_ascii_lowercase();
        let index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut candidate_counts: HashMap<String, usize> = HashMap::new();

        for id in &index.order_desc_ids {
            let Some(indexed) = index.by_id.get(id) else {
                continue;
            };
            let Some(candidate) = bowl_name_from_tags(&indexed.record.tags) else {
                continue;
            };
            if bowl_suggestion_match_rank(&candidate, &normalized_query).is_some() {
                *candidate_counts.entry(candidate).or_insert(0) += 1;
            }
        }

        let mut suggestions: Vec<(String, u8, usize)> = candidate_counts
            .into_iter()
            .filter_map(|(candidate, count)| {
                let rank = bowl_suggestion_match_rank(&candidate, &normalized_query)?;
                Some((candidate, rank, count))
            })
            .collect();

        suggestions.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| left.0.len().cmp(&right.0.len()))
                .then_with(|| left.0.cmp(&right.0))
        });

        suggestions
            .into_iter()
            .take(limit)
            .map(|(candidate, _, _)| candidate)
            .collect()
    }

    pub fn delete_item(&self, id: i64) -> Result<bool> {
        let conn = self.open()?;
        let deleted = conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        if deleted > 0 {
            self.remove_index_record(id);
        }
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

        let (forced_type, forced_tags) =
            classify_clipboard_text_with_mode(&plaintext, SecretClassificationMode::ForceSecret);
        let existing_tags: Vec<String> =
            serde_json::from_str(&existing_tags_json).unwrap_or_default();
        let merged = merge_reclassified_tags(&existing_tags, forced_tags, false);
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
        self.sync_index_record_from_db(id)?;
        Ok(true)
    }

    pub fn unmark_item_as_secret(&self, id: i64) -> Result<bool> {
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

        let (restored_type, restored_tags) =
            classify_clipboard_text_with_mode(&plaintext, SecretClassificationMode::Ignore);
        let existing_tags: Vec<String> =
            serde_json::from_str(&existing_tags_json).unwrap_or_default();
        let merged = merge_reclassified_tags(&existing_tags, restored_tags, true);
        let merged_tags_json = serde_json::to_string(&merged)?;

        let should_update = existing_item_type != restored_type.as_str() || is_encrypted != 0;
        if !should_update && existing_tags_json == merged_tags_json {
            tx.rollback()?;
            return Ok(false);
        }

        tx.execute(
            "UPDATE clipboard_items
             SET item_type = ?1, content = ?2, is_encrypted = 0, tags = ?3
             WHERE id = ?4",
            params![restored_type.as_str(), plaintext, merged_tags_json, id],
        )?;

        tx.commit()?;
        self.sync_index_record_from_db(id)?;
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
        self.sync_index_record_from_db(id)?;
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
        self.sync_index_record_from_db(id)?;
        Ok(true)
    }

    pub fn set_item_bowl(&self, id: i64, bowl_name: Option<&str>) -> Result<bool> {
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
        let mut filtered: Vec<String> = tags.into_iter().filter(|tag| !is_bowl_tag(tag)).collect();
        if let Some(normalized_bowl) = bowl_name.and_then(normalize_bowl_name) {
            filtered.push(make_bowl_tag(&normalized_bowl));
        }

        filtered.sort_unstable();
        filtered.dedup();
        let next_tags_json = serde_json::to_string(&filtered)?;
        if next_tags_json == existing_tags_json {
            return Ok(false);
        }

        conn.execute(
            "UPDATE clipboard_items SET tags = ?1 WHERE id = ?2",
            params![next_tags_json, id],
        )?;
        self.sync_index_record_from_db(id)?;
        Ok(true)
    }

    pub fn items_in_bowl(&self, bowl_name: &str) -> Vec<ClipboardRecord> {
        let Some(normalized_bowl) = normalize_bowl_name(bowl_name) else {
            return Vec::new();
        };

        let index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut output = Vec::new();
        for id in &index.order_desc_ids {
            let Some(indexed) = index.by_id.get(id) else {
                continue;
            };
            let Some(candidate) = bowl_name_from_tags(&indexed.record.tags) else {
                continue;
            };
            if candidate.eq_ignore_ascii_case(&normalized_bowl) {
                output.push(indexed.record.clone());
            }
        }

        output
    }

    pub fn import_bowl_bundle(&self, bundle: &BowlExportBundle) -> Result<BowlImportSummary> {
        if bundle.kind.trim() != "pasta-bowl" {
            return Err(anyhow!("unsupported bowl bundle kind"));
        }
        if bundle.version != 1 {
            return Err(anyhow!("unsupported bowl bundle version"));
        }

        let normalized_bowl =
            normalize_bowl_name(&bundle.bowl).context("bowl name is missing or invalid")?;
        let mut imported_count = 0_usize;
        for item in &bundle.items {
            if self.upsert_imported_bowl_item(item, &normalized_bowl)? {
                imported_count += 1;
            }
        }

        Ok(BowlImportSummary {
            bowl: normalized_bowl,
            imported_count,
        })
    }

    pub fn upsert_item_parameter(&self, id: i64, name: &str, target: &str) -> Result<bool> {
        let normalized_name = name.trim();
        let normalized_target = target.trim();
        if normalized_name.is_empty() || normalized_target.is_empty() {
            return Ok(false);
        }

        let conn = self.open()?;
        let parameters_json: Option<String> = conn
            .query_row(
                "SELECT parameters FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(parameters_json) = parameters_json else {
            return Ok(false);
        };

        let mut parameters: Vec<ClipboardParameter> =
            serde_json::from_str(&parameters_json).unwrap_or_default();
        let mut changed = false;

        if let Some(existing) = parameters
            .iter_mut()
            .find(|parameter| parameter.name.eq_ignore_ascii_case(normalized_name))
        {
            if existing.target != normalized_target || existing.name != normalized_name {
                existing.name = normalized_name.to_owned();
                existing.target = normalized_target.to_owned();
                changed = true;
            }
        } else if let Some(existing) = parameters
            .iter_mut()
            .find(|parameter| parameter.target == normalized_target)
        {
            if existing.name != normalized_name {
                existing.name = normalized_name.to_owned();
                changed = true;
            }
        } else {
            parameters.push(ClipboardParameter {
                name: normalized_name.to_owned(),
                target: normalized_target.to_owned(),
            });
            changed = true;
        }

        if !changed {
            return Ok(false);
        }

        parameters.sort_unstable_by_key(|parameter| parameter.name.to_ascii_lowercase());
        let parameters_json = serde_json::to_string(&parameters)?;
        conn.execute(
            "UPDATE clipboard_items SET parameters = ?1 WHERE id = ?2",
            params![parameters_json, id],
        )?;
        self.sync_index_record_from_db(id)?;
        Ok(true)
    }

    pub fn upsert_item_description(&self, id: i64, description: &str) -> Result<bool> {
        let normalized = description.trim().to_owned();
        let conn = self.open()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT description FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(existing) = existing else {
            return Ok(false);
        };

        if existing == normalized {
            return Ok(false);
        }

        conn.execute(
            "UPDATE clipboard_items SET description = ?1 WHERE id = ?2",
            params![normalized, id],
        )?;
        self.sync_index_record_from_db(id)?;
        Ok(true)
    }

    fn upsert_imported_bowl_item(&self, item: &BowlExportItem, bowl_name: &str) -> Result<bool> {
        let item_type = ClipboardItemType::from_str(item.item_type.trim());
        if item_type == ClipboardItemType::Password {
            return Ok(false);
        }

        let (rehydrated_content, parameters) = rehydrate_bowl_export_item(item);
        let normalized_content = rehydrated_content.trim();
        if normalized_content.is_empty() {
            return Err(anyhow!("imported snippet content is empty"));
        }

        let content_hash = content_hash(normalized_content);
        let mut tags = tags_without_bowl(&item.tags);
        let mut seen: HashSet<String> = tags.iter().map(|tag| tag.to_ascii_lowercase()).collect();
        let bowl_tag = make_bowl_tag(bowl_name);
        if seen.insert(bowl_tag.to_ascii_lowercase()) {
            tags.push(bowl_tag);
        }

        let tags_json = serde_json::to_string(&tags)?;
        let parameters_json = serde_json::to_string(&parameters)?;
        let description = item.description.trim().to_owned();
        let (stored_content, is_encrypted) = if item_type == ClipboardItemType::Password {
            (self.crypto.encrypt(normalized_content)?, 1_i64)
        } else {
            (normalized_content.to_owned(), 0_i64)
        };

        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let existing_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM clipboard_items WHERE content_hash = ?1 ORDER BY id DESC LIMIT 1",
                params![content_hash],
                |row| row.get(0),
            )
            .optional()?;

        let record_id = if let Some(existing_id) = existing_id {
            tx.execute(
                "UPDATE clipboard_items
                 SET item_type = ?1, content = ?2, is_encrypted = ?3, tags = ?4, parameters = ?5, description = ?6
                 WHERE id = ?7",
                params![
                    item_type.as_str(),
                    stored_content,
                    is_encrypted,
                    tags_json,
                    parameters_json,
                    description,
                    existing_id
                ],
            )?;
            existing_id
        } else {
            let created_at = Utc::now().to_rfc3339();
            tx.execute(
                "INSERT INTO clipboard_items (item_type, content, is_encrypted, tags, parameters, description, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    item_type.as_str(),
                    stored_content,
                    is_encrypted,
                    tags_json,
                    parameters_json,
                    description,
                    content_hash,
                    created_at
                ],
            )?;
            tx.last_insert_rowid()
        };

        tx.commit()?;
        self.sync_index_record_from_db(record_id)?;
        Ok(true)
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path).context("unable to open sqlite database")?;
        // Wait up to 2s when another connection is writing before returning SQLITE_BUSY.
        // This is necessary because the clipboard watcher, search worker, and UI thread
        // can all touch the DB concurrently.
        let _ = conn.busy_timeout(Duration::from_millis(2_000));
        Ok(conn)
    }

    fn apply_persistent_pragmas(&self) -> Result<()> {
        let conn = self.open()?;
        // WAL persists in the database file; set once so subsequent connections inherit it.
        // synchronous=NORMAL pairs safely with WAL and avoids the fsync cost of FULL.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Ok(())
    }

    pub fn clear_all_items(&self) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM clipboard_items", [])?;
        if let Ok(mut index) = self.memory_index.lock() {
            *index = MemorySearchIndex::default();
        }
        if let Ok(mut cache) = self.semantic_embedding_cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.neural_embedding_cache.lock() {
            cache.clear();
        }
        Ok(())
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
                parameters TEXT NOT NULL DEFAULT '[]',
                description TEXT NOT NULL DEFAULT '',
                content_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);",
        )?;
        self.ensure_parameters_column(&conn)?;
        self.ensure_description_column(&conn)?;
        Ok(())
    }

    fn ensure_parameters_column(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(clipboard_items)")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "parameters" {
                return Ok(());
            }
        }

        conn.execute(
            "ALTER TABLE clipboard_items ADD COLUMN parameters TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
        Ok(())
    }

    fn ensure_description_column(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(clipboard_items)")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "description" {
                return Ok(());
            }
        }

        conn.execute(
            "ALTER TABLE clipboard_items ADD COLUMN description TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        Ok(())
    }

    fn rebuild_memory_index(&self) -> Result<()> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, item_type, content, is_encrypted, tags, parameters, description, created_at, content_hash
             FROM clipboard_items
             ORDER BY id DESC",
        )?;
        let mut rows = stmt.query([])?;

        let mut rebuilt = MemorySearchIndex::default();
        while let Some(row) = rows.next()? {
            let Some(indexed) = self.indexed_record_from_row(row)? else {
                continue;
            };
            let id = indexed.record.id;
            rebuilt.order_desc_ids.push(id);
            rebuilt.by_id.insert(id, indexed);
        }

        let mut index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *index = rebuilt;
        Ok(())
    }

    fn sync_index_record_from_db(&self, id: i64) -> Result<()> {
        let conn = self.open()?;
        let indexed = self.load_indexed_record_by_id(&conn, id)?;
        if let Some(indexed) = indexed {
            self.upsert_index_record(indexed);
        } else {
            self.remove_index_record(id);
        }
        Ok(())
    }

    fn load_indexed_record_by_id(
        &self,
        conn: &Connection,
        id: i64,
    ) -> Result<Option<IndexedRecord>> {
        let result: Option<Option<IndexedRecord>> = conn
            .query_row(
                "SELECT id, item_type, content, is_encrypted, tags, parameters, description, created_at, content_hash
                 FROM clipboard_items
                 WHERE id = ?1",
                params![id],
                |row| self.indexed_record_from_row(row),
            )
            .optional()?;
        Ok(result.flatten())
    }

    fn indexed_record_from_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<Option<IndexedRecord>> {
        let id: i64 = row.get(0)?;
        let item_type = ClipboardItemType::from_str(row.get::<_, String>(1)?.as_str());
        let mut content: String = row.get(2)?;
        let is_encrypted: i64 = row.get(3)?;
        let tags_json: String = row.get(4)?;
        let parameters_json: String = row.get(5)?;
        let description: String = row.get(6)?;
        let created_at: String = row.get(7)?;
        let content_hash: String = row.get(8)?;

        if is_encrypted == 1 {
            let Ok(decrypted) = self.crypto.decrypt(&content) else {
                return Ok(None);
            };
            content = decrypted;
        }

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let parameters: Vec<ClipboardParameter> =
            serde_json::from_str(&parameters_json).unwrap_or_default();
        let content_lower = content.to_lowercase();
        let description_lower = description.to_lowercase();
        let fuzzy_search_text = if description_lower.is_empty() {
            bounded_text_prefix(&content_lower, SEARCH_FUZZY_TEXT_LIMIT).to_owned()
        } else {
            format!(
                "{} {}",
                bounded_text_prefix(&content_lower, SEARCH_FUZZY_TEXT_LIMIT),
                bounded_text_prefix(&description_lower, SEARCH_FUZZY_TEXT_LIMIT.min(512))
            )
        };
        let search_tokens: Vec<String> = tokenize_search_terms(&fuzzy_search_text)
            .into_iter()
            .map(|s| s.to_owned())
            .collect();
        Ok(Some(IndexedRecord {
            record: ClipboardRecord {
                id,
                item_type,
                content,
                description,
                tags,
                parameters,
                created_at,
            },
            content_hash,
            content_lower,
            description_lower,
            search_tokens,
        }))
    }

    fn upsert_index_record(&self, indexed: IndexedRecord) {
        let id = indexed.record.id;
        let mut index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !index.by_id.contains_key(&id) {
            index.order_desc_ids.push(id);
        }
        index.by_id.insert(id, indexed);
        index
            .order_desc_ids
            .sort_unstable_by(|left, right| right.cmp(left));
    }

    fn remove_index_record(&self, id: i64) {
        let mut index = self
            .memory_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if index.by_id.remove(&id).is_some() {
            index.order_desc_ids.retain(|existing| *existing != id);
        }
    }

    fn cached_semantic_embedding(
        &self,
        cache_key: &str,
        content: &str,
        seed_terms: &[String],
    ) -> Vec<f32> {
        if let Ok(cache) = self.semantic_embedding_cache.lock()
            && let Some(existing) = cache.get(cache_key)
        {
            return existing.clone();
        }

        let embedding = semantic_embedding(
            bounded_text_prefix(content, SEMANTIC_SOURCE_TEXT_LIMIT),
            seed_terms,
        );
        if let Ok(mut cache) = self.semantic_embedding_cache.lock() {
            if cache.len() >= SEMANTIC_EMBEDDING_CACHE_MAX {
                cache.clear();
            }
            cache.insert(cache_key.to_owned(), embedding.clone());
        }
        embedding
    }

    pub fn set_neural_embedder(&self, embedder: Arc<NeuralEmbedder>) {
        if let Ok(mut guard) = self.neural_embedder.lock() {
            *guard = Some(embedder);
        }
    }

    pub fn get_neural_embedder(&self) -> Option<Arc<NeuralEmbedder>> {
        self.neural_embedder
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    fn cached_neural_embedding(
        &self,
        cache_key: &str,
        content: &str,
        seed_terms: &[String],
        embedder: Option<&Arc<NeuralEmbedder>>,
    ) -> Vec<f32> {
        if let Ok(cache) = self.neural_embedding_cache.lock()
            && let Some(existing) = cache.get(cache_key)
        {
            return existing.clone();
        }

        let Some(embedder) = embedder else {
            return NeuralEmbedder::zero_vector();
        };

        let embedding = embedder.embed(
            bounded_text_prefix(content, SEMANTIC_SOURCE_TEXT_LIMIT),
            seed_terms,
        );
        if let Ok(mut cache) = self.neural_embedding_cache.lock() {
            if cache.len() >= SEMANTIC_EMBEDDING_CACHE_MAX {
                cache.clear();
            }
            cache.insert(cache_key.to_owned(), embedding.clone());
        }
        embedding
    }
}

fn primary_db_path(app_dir_name: &str) -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .context("unable to determine data directory")?
        .join(app_dir_name);
    fs::create_dir_all(&data_dir).context("unable to create data directory")?;

    let db_path = data_dir.join("clipboard.db");
    verify_sqlite_path(&db_path)?;
    Ok(db_path)
}

fn sweep_orphaned_fallback_dbs(data_dir: &std::path::Path, current_pid: u32) {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return;
    };
    let current_marker = format!("-{current_pid}.db");
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.starts_with("clipboard-fallback-") {
            continue;
        }
        // Keep the file that belongs to the current process; SQLite may also create
        // -wal and -shm sidecars which we intentionally leave alone for the live db
        // and prune for stale ones below.
        if name.ends_with(&current_marker)
            || name.ends_with(&format!("-{current_pid}.db-wal"))
            || name.ends_with(&format!("-{current_pid}.db-shm"))
        {
            continue;
        }
        let _ = fs::remove_file(entry.path());
    }
}

fn fallback_db_path(app_dir_name: &str) -> Result<PathBuf> {
    let candidate_roots = [
        dirs::cache_dir(),
        dirs::data_local_dir(),
        dirs::home_dir().map(|path| path.join(".cache")),
        Some(std::env::temp_dir()),
        Some(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    ];

    for data_dir in candidate_roots
        .into_iter()
        .flatten()
        .map(|root| root.join(app_dir_name))
    {
        if fs::create_dir_all(&data_dir).is_err() {
            continue;
        }

        let db_path = data_dir.join(format!("clipboard-fallback-{}.db", std::process::id()));
        if verify_sqlite_path(&db_path).is_ok() {
            return Ok(db_path);
        }
    }

    Err(anyhow!("unable to create fallback data directory"))
}

fn verify_sqlite_path(path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("unable to create {:?}", parent))?;
    }

    Connection::open(path)
        .with_context(|| format!("unable to open sqlite database at {:?}", path))?
        .close()
        .map_err(|(_, err)| anyhow!(err))
        .context("unable to close sqlite database probe")?;

    Ok(())
}

pub(crate) fn parse_search_query(query: &str) -> SearchQuery {
    let normalized = query.trim().to_ascii_lowercase();
    if !normalized.starts_with(TAG_ONLY_PREFIX) {
        return SearchQuery::Default {
            effective_query: normalized,
        };
    }

    let effective_query = normalized
        .trim_start_matches(TAG_ONLY_PREFIX)
        .trim_start()
        .to_owned();
    if let Some(bowl_query) = search_command_argument(&effective_query, SEARCH_COMMAND_BOWL) {
        return SearchQuery::Bowl { bowl_query };
    }
    if let Some(bowl_query) = search_command_argument(&effective_query, SEARCH_COMMAND_EXPORT) {
        return SearchQuery::ExportBowl { bowl_query };
    }

    SearchQuery::TagOnly { effective_query }
}

fn search_command_argument(query: &str, command: &str) -> Option<String> {
    if query == command {
        return Some(String::new());
    }
    let remainder = query.strip_prefix(command)?;
    let first = remainder.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(remainder.trim().to_owned())
}

fn is_bowl_tag(tag: &str) -> bool {
    tag.trim().to_ascii_lowercase().starts_with(BOWL_TAG_PREFIX)
}

fn make_bowl_tag(bowl_name: &str) -> String {
    format!("BOWL:{bowl_name}")
}

pub(crate) fn bowl_name_from_tags(tags: &[String]) -> Option<String> {
    tags.iter().find_map(|tag| {
        let trimmed = tag.trim();
        let lower = trimmed.to_ascii_lowercase();
        let stripped = lower.strip_prefix(BOWL_TAG_PREFIX)?;
        if stripped.is_empty() {
            return None;
        }

        Some(trimmed[trimmed.len() - stripped.len()..].to_owned())
    })
}

pub(crate) fn tags_without_bowl(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|tag| !is_bowl_tag(tag))
        .cloned()
        .collect()
}

fn normalize_bowl_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("bowl:")
        .or_else(|| trimmed.strip_prefix("BOWL:"))
        .unwrap_or(trimmed);
    normalize_custom_tag(stripped)
}

fn bowl_parameter_placeholder(name: &str) -> String {
    format!("{{{{{}}}}}", name.trim())
}

fn rehydrate_bowl_export_item(item: &BowlExportItem) -> (String, Vec<ClipboardParameter>) {
    if item.parameters.is_empty() {
        return (item.content.clone(), Vec::new());
    }

    let mut output = item.content.clone();
    let mut replacements = Vec::new();
    let mut parameters = Vec::new();

    for (idx, parameter) in item.parameters.iter().enumerate() {
        let name = parameter.name.trim();
        if name.is_empty() {
            continue;
        }

        let placeholder = bowl_parameter_placeholder(name);
        let mut marker = format!("\u{001F}PASTA_BOWL_PARAM_{idx}\u{001E}");
        while output.contains(&marker) {
            marker.push('_');
        }
        output = output.replace(&placeholder, &marker);

        let default_value = parameter.default_value.trim().to_owned();
        let target = if default_value.is_empty() {
            placeholder.clone()
        } else {
            default_value.clone()
        };
        replacements.push((marker, target.clone()));
        parameters.push(ClipboardParameter {
            name: name.to_owned(),
            target,
        });
    }

    for (marker, replacement) in replacements {
        output = output.replace(&marker, &replacement);
    }

    (output, parameters)
}

fn record_matches_bowl(record: &ClipboardRecord, bowl_query: &str) -> bool {
    let Some(candidate) = bowl_name_from_tags(&record.tags) else {
        return false;
    };

    let normalized_candidate = candidate.to_ascii_lowercase();
    if bowl_query.is_empty() {
        return true;
    }

    normalized_candidate.contains(bowl_query)
        || (bowl_query.len() >= 3
            && bowl_query.contains(&normalized_candidate)
            && normalized_candidate.len() >= 2)
        || fuzzy_token_match(bowl_query, &normalized_candidate)
}

struct TagSearchAutocompleteContext {
    completed_terms: HashSet<String>,
    fragment: String,
}

fn tag_search_autocomplete_context(query: &str) -> Option<TagSearchAutocompleteContext> {
    let trimmed_start = query.trim_start();
    let effective_query = trimmed_start.strip_prefix(TAG_ONLY_PREFIX)?.trim_start();
    let trimmed_end = effective_query.trim_end();
    if trimmed_end == SEARCH_COMMAND_BOWL
        || trimmed_end == SEARCH_COMMAND_EXPORT
        || trimmed_end.starts_with(&format!("{SEARCH_COMMAND_BOWL} "))
        || trimmed_end.starts_with(&format!("{SEARCH_COMMAND_EXPORT} "))
    {
        return None;
    }
    let ends_with_whitespace = effective_query
        .chars()
        .last()
        .is_some_and(|ch| ch.is_whitespace());
    let mut terms: Vec<String> = effective_query
        .split_whitespace()
        .map(|term| term.trim_start_matches(TAG_ONLY_PREFIX).trim().to_owned())
        .filter(|term| !term.is_empty())
        .collect();

    let fragment = if ends_with_whitespace {
        String::new()
    } else {
        terms.pop().unwrap_or_default()
    };

    Some(TagSearchAutocompleteContext {
        completed_terms: terms.into_iter().collect(),
        fragment,
    })
}

fn tag_search_suggestion_match_rank(
    candidate: &str,
    context: &TagSearchAutocompleteContext,
) -> Option<u8> {
    if candidate.len() < 2 || context.completed_terms.contains(candidate) {
        return None;
    }

    if context.fragment.is_empty() {
        return Some(3);
    }
    if candidate == context.fragment {
        return None;
    }
    if candidate.starts_with(&context.fragment) {
        return Some(0);
    }
    if candidate.contains(&context.fragment) {
        return Some(1);
    }
    if fuzzy_token_match(&context.fragment, candidate) {
        return Some(2);
    }

    None
}

fn bowl_suggestion_match_rank(candidate: &str, fragment: &str) -> Option<u8> {
    let normalized_candidate = candidate.to_ascii_lowercase();
    if normalized_candidate.len() < 2 {
        return None;
    }
    if fragment.is_empty() {
        return Some(3);
    }
    if normalized_candidate == fragment {
        return None;
    }
    if normalized_candidate.starts_with(fragment) {
        return Some(0);
    }
    if normalized_candidate.contains(fragment) {
        return Some(1);
    }
    if fuzzy_token_match(fragment, &normalized_candidate) {
        return Some(2);
    }

    None
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

    fn ephemeral() -> Self {
        let mut key = [0_u8; 32];
        OsRng.fill_bytes(&mut key);
        Self { key }
    }
}

fn classify_clipboard_text(text: &str) -> (ClipboardItemType, Vec<String>) {
    classify_clipboard_text_with_mode(text, SecretClassificationMode::Auto)
}

fn classify_clipboard_text_with_hint(
    text: &str,
    force_secret: bool,
) -> (ClipboardItemType, Vec<String>) {
    classify_clipboard_text_with_mode(
        text,
        if force_secret {
            SecretClassificationMode::ForceSecret
        } else {
            SecretClassificationMode::Auto
        },
    )
}

fn classify_clipboard_text_with_mode(
    text: &str,
    mode: SecretClassificationMode,
) -> (ClipboardItemType, Vec<String>) {
    let mut tags = Vec::new();
    let looks_base64 = looks_like_base64_blob(text);
    let secret_detection_enabled = matches!(
        mode,
        SecretClassificationMode::Auto | SecretClassificationMode::ForceSecret
    );

    let item_type = if secret_detection_enabled && !looks_base64 && looks_like_password(text) {
        tags.push("sensitive".to_owned());
        tags.push("secret".to_owned());
        tags.push("pass".to_owned());
        ClipboardItemType::Password
    } else if secret_detection_enabled && !looks_base64 && looks_like_high_entropy_secret(text) {
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
    if looks_base64 {
        enriched.insert("base64".to_owned());
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
    if looks_like_ip_address(text) {
        enriched.insert("ip".to_owned());
    }
    if let Some(language) = detect_language_tag(item_type, text) {
        enriched.insert(language.to_owned());
        enriched.insert(format!("lang:{language}"));

        // Domain-specific structural tags derived from detected language.
        match language {
            "dockerfile" => {
                enriched.insert("docker".to_owned());
            }
            "yaml" => {
                let lower = text.to_ascii_lowercase();
                if (lower.contains("apiversion:") || lower.contains("apiversion :"))
                    && (lower.contains("kind:") || lower.contains("kind :"))
                {
                    enriched.insert("k8s".to_owned());
                    enriched.insert("kubernetes".to_owned());
                }
                if lower.contains("ansible")
                    || (lower.contains("hosts:") && lower.contains("tasks:"))
                {
                    enriched.insert("ansible".to_owned());
                }
            }
            _ => {}
        }
    }
    // Terraform / HCL detection.
    {
        let lower = text.to_ascii_lowercase();
        if (lower.contains("resource \"") && lower.contains("provider \""))
            || (lower.contains("resource \"") && lower.contains("terraform {"))
            || (lower.contains("variable \"") && lower.contains("type ="))
        {
            enriched.insert("terraform".to_owned());
            enriched.insert("hcl".to_owned());
        }
    }
    // JWT token detection.
    if looks_like_jwt(text) {
        enriched.insert("jwt".to_owned());
        enriched.insert("token".to_owned());
    }

    if mode == SecretClassificationMode::ForceSecret {
        enriched.retain(|tag| {
            let lower = tag.to_ascii_lowercase();
            !is_type_classification_tag(&lower)
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
        if mode == SecretClassificationMode::ForceSecret {
            ClipboardItemType::Password
        } else {
            item_type
        },
        ordered,
    )
}

fn is_type_classification_tag(tag: &str) -> bool {
    matches!(tag, "text" | "code" | "command" | "password") || tag.starts_with("type:")
}

fn is_secret_classification_tag(tag: &str) -> bool {
    matches!(
        tag,
        "sensitive" | "secret" | "pass" | "password" | "high_entropy" | "token"
    )
}

fn merge_reclassified_tags(
    existing_tags: &[String],
    mut derived_tags: Vec<String>,
    strip_secret_markers: bool,
) -> Vec<String> {
    let mut seen: HashSet<String> = derived_tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect();

    for tag in existing_tags {
        let lower = tag.to_ascii_lowercase();
        if is_type_classification_tag(&lower)
            || (strip_secret_markers && is_secret_classification_tag(&lower))
        {
            continue;
        }
        if seen.insert(lower) {
            derived_tags.push(tag.clone());
        }
    }

    derived_tags.sort_unstable_by_key(|tag| tag.to_ascii_lowercase());
    derived_tags
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

fn looks_like_base64_blob(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 16 || trimmed.contains(char::is_whitespace) || trimmed.len() % 4 != 0 {
        return false;
    }

    if !trimmed.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-' | b'_')
    }) {
        return false;
    }

    BASE64.decode(trimmed.as_bytes()).is_ok()
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

fn looks_like_ip_address(text: &str) -> bool {
    let value = text.trim();
    if value.contains('\n') || value.is_empty() {
        return false;
    }
    // Strip optional port suffix for matching (e.g., "192.168.1.1:8080").
    let host = value.split(':').next().unwrap_or(value);
    // IPv4: four octets separated by dots, each 0-255.
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() == 4 {
        return parts.iter().all(|p| p.parse::<u8>().is_ok());
    }
    // IPv6: contains at least two colons and only hex digits, colons, dots (for mapped v4).
    if value.contains("::") || value.matches(':').count() >= 2 {
        let stripped = value.trim_start_matches('[').trim_end_matches(']');
        return stripped
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == ':' || ch == '.');
    }
    false
}

fn looks_like_jwt(text: &str) -> bool {
    let value = text.trim();
    if value.contains('\n') || value.contains(' ') {
        return false;
    }
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    // Each part must be non-empty and look like base64url.
    parts.iter().all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '=')
    })
}

fn detect_language_tag(item_type: ClipboardItemType, text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(fenced_language) = detect_language_from_fenced_block(trimmed) {
        return Some(fenced_language);
    }

    let lower = trimmed.to_ascii_lowercase();

    if item_type == ClipboardItemType::Command
        || looks_like_shell(trimmed)
        || looks_like_shell_prompt_snippet(&lower)
    {
        return Some("bash");
    }
    if looks_like_json(trimmed) {
        return Some("json");
    }
    if looks_like_toml(trimmed) {
        return Some("toml");
    }
    if looks_like_yaml(trimmed) {
        return Some("yaml");
    }
    if looks_like_html(&lower) {
        return Some("html");
    }
    if looks_like_xml(trimmed, &lower) {
        return Some("xml");
    }
    if looks_like_css(&lower) {
        return Some("css");
    }
    if looks_like_sql(&lower) {
        return Some("sql");
    }
    if looks_like_dockerfile(&lower) {
        return Some("dockerfile");
    }
    if looks_like_makefile(trimmed) {
        return Some("makefile");
    }
    if looks_like_markdown(trimmed, &lower) {
        return Some("markdown");
    }
    if let Some(language) = detect_programming_language(&lower) {
        return Some(language);
    }
    if item_type == ClipboardItemType::Code {
        return Some("code");
    }

    None
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn detect_language_from_fenced_block(text: &str) -> Option<&'static str> {
    for line in text.lines().take(8) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let label = trimmed.strip_prefix("```")?;
        let raw = label
            .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '{' | '}'))
            .next()
            .unwrap_or("")
            .trim();
        return normalize_fenced_language_alias(raw).or(Some("markdown"));
    }

    None
}

fn normalize_fenced_language_alias(raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    match normalized.as_str() {
        "bash" | "sh" | "zsh" | "shell" | "fish" | "console" => Some("bash"),
        "rs" | "rust" => Some("rust"),
        "zig" => Some("zig"),
        "go" | "golang" => Some("go"),
        "ts" | "tsx" | "typescript" => Some("typescript"),
        "js" | "jsx" | "javascript" | "node" => Some("javascript"),
        "py" | "python" => Some("python"),
        "java" => Some("java"),
        "kt" | "kts" | "kotlin" => Some("kotlin"),
        "swift" => Some("swift"),
        "cs" | "csharp" | "c#" | "dotnet" => Some("csharp"),
        "cpp" | "cxx" | "cc" | "c++" => Some("cpp"),
        "c" => Some("c"),
        "php" => Some("php"),
        "rb" | "ruby" => Some("ruby"),
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        "xml" => Some("xml"),
        "html" => Some("html"),
        "css" => Some("css"),
        "sql" => Some("sql"),
        "md" | "markdown" => Some("markdown"),
        "dockerfile" => Some("dockerfile"),
        "makefile" | "make" | "mk" => Some("makefile"),
        _ => None,
    }
}

fn looks_like_shell(text: &str) -> bool {
    text.starts_with("#!/bin/bash")
        || text.starts_with("#!/usr/bin/env bash")
        || text.starts_with("#!/bin/zsh")
        || text.starts_with("#!/usr/bin/env zsh")
        || text.starts_with("#!/bin/sh")
        || text.starts_with("#!/usr/bin/env sh")
        || text.starts_with("#!/usr/bin/env fish")
}

fn looks_like_shell_prompt_snippet(lower: &str) -> bool {
    let mut prompt_lines = 0_usize;
    let mut command_lines = 0_usize;
    let mut meaningful_lines = 0_usize;

    for line in lower.lines().take(24) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        meaningful_lines += 1;

        if trimmed.starts_with("$ ") || trimmed.starts_with("% ") {
            prompt_lines += 1;
            command_lines += 1;
            continue;
        }

        if contains_any(
            trimmed,
            &[
                "sudo ", "git ", "docker ", "kubectl ", "brew ", "npm ", "pnpm ", "yarn ",
                "cargo ", "./", "chmod ", "chown ", "ssh ", "scp ",
            ],
        ) {
            command_lines += 1;
        }
    }

    (prompt_lines >= 1 && meaningful_lines <= 16) || command_lines >= 2
}

fn looks_like_markdown(trimmed: &str, lower: &str) -> bool {
    if lower.contains("```") {
        return true;
    }

    let mut heading_lines = 0_usize;
    let mut list_lines = 0_usize;
    let mut quote_lines = 0_usize;
    let mut link_lines = 0_usize;

    for line in trimmed.lines().take(140) {
        let candidate = line.trim_start();
        if candidate.is_empty() {
            continue;
        }

        if candidate.starts_with("# ")
            || candidate.starts_with("## ")
            || candidate.starts_with("### ")
            || candidate.starts_with("#### ")
        {
            heading_lines += 1;
        }
        if candidate.starts_with("- ")
            || candidate.starts_with("* ")
            || candidate.starts_with("+ ")
            || starts_with_markdown_numbered_list(candidate)
        {
            list_lines += 1;
        }
        if candidate.starts_with("> ") {
            quote_lines += 1;
        }
        if candidate.contains("](") && candidate.contains('[') {
            link_lines += 1;
        }
    }

    heading_lines >= 2
        || (heading_lines >= 1 && (list_lines >= 1 || quote_lines >= 1 || link_lines >= 1))
        || (list_lines >= 2 && (quote_lines >= 1 || link_lines >= 1))
}

fn starts_with_markdown_numbered_list(line: &str) -> bool {
    let mut digit_count = 0_usize;
    for (ix, ch) in line.char_indices() {
        if ch.is_ascii_digit() {
            digit_count += 1;
            continue;
        }
        if ch == '.' && digit_count > 0 {
            let remainder = line.get(ix + 1..).unwrap_or("").trim_start();
            return !remainder.is_empty();
        }
        return false;
    }
    false
}

fn looks_like_html(lower: &str) -> bool {
    lower.contains("<!doctype html")
        || lower.contains("<html")
        || (lower.contains("</")
            && contains_any(
                lower,
                &[
                    "<div", "<span", "<body", "<head", "<script", "<style", "<section",
                ],
            ))
}

fn looks_like_xml(trimmed: &str, lower: &str) -> bool {
    if looks_like_html(lower) {
        return false;
    }
    if lower.starts_with("<?xml") {
        return true;
    }
    if !trimmed.starts_with('<') || !lower.contains("</") {
        return false;
    }

    let mut paired_lines = 0_usize;
    for line in trimmed.lines().take(120) {
        let candidate = line.trim();
        if candidate.starts_with('<')
            && !candidate.starts_with("</")
            && !candidate.starts_with("<!--")
            && candidate.contains("</")
        {
            paired_lines += 1;
        }
    }

    paired_lines >= 1
}

fn looks_like_css(lower: &str) -> bool {
    if !(lower.contains('{') && lower.contains('}')) {
        return false;
    }

    let property_hits = count_contains(
        lower,
        &[
            "color:",
            "background:",
            "display:",
            "margin:",
            "padding:",
            "border:",
            "font-",
            "width:",
            "height:",
            "position:",
            "grid-",
            "flex",
        ],
    );
    let selector_like = contains_any(lower, &[".", "#", ":root", "@media", "body", "html"]);
    property_hits >= 2 && selector_like
}

fn looks_like_sql(lower: &str) -> bool {
    let statement_hits = count_contains(
        lower,
        &[
            "select ",
            "insert into ",
            "update ",
            "delete from ",
            "create table ",
            "alter table ",
            "drop table ",
            "with ",
            "join ",
        ],
    );

    let clause_hits = count_contains(
        lower,
        &[
            " from ",
            " where ",
            " values ",
            " set ",
            " order by ",
            " group by ",
        ],
    );

    (statement_hits >= 1 && clause_hits >= 1) || (statement_hits >= 2 && lower.contains(';'))
}

fn looks_like_dockerfile(lower: &str) -> bool {
    let mut directive_hits = 0_i32;
    let mut content_lines = 0_i32;
    let directives = [
        "from ",
        "run ",
        "copy ",
        "add ",
        "cmd ",
        "entrypoint ",
        "workdir ",
        "env ",
        "arg ",
        "expose ",
        "user ",
    ];

    for line in lower.lines().take(80) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        content_lines += 1;
        if directives
            .iter()
            .any(|directive| trimmed.starts_with(directive))
        {
            directive_hits += 1;
        }
    }

    directive_hits >= 2 && content_lines >= 2
}

fn looks_like_makefile(text: &str) -> bool {
    if text.contains(".PHONY:") {
        return true;
    }

    let mut target_lines = 0_usize;
    let mut recipe_lines = 0_usize;
    for line in text.lines().take(120) {
        if line.starts_with('\t') {
            recipe_lines += 1;
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.contains(':')
            && !trimmed.contains('=')
            && !trimmed.starts_with("http")
            && !trimmed.starts_with("https")
        {
            target_lines += 1;
        }
    }

    target_lines >= 1 && recipe_lines >= 1
}

fn count_contains(haystack: &str, needles: &[&str]) -> i32 {
    needles
        .iter()
        .filter(|needle| haystack.contains(**needle))
        .count() as i32
}

fn detect_programming_language(lower: &str) -> Option<&'static str> {
    let mut scores = HashMap::new();

    scores.insert(
        "rust",
        count_contains(
            lower,
            &[
                "use std::",
                "use crate::",
                "println!(",
                "eprintln!(",
                "pub(crate)",
                "#[derive(",
                "impl ",
                "trait ",
                "enum ",
                "match ",
                "let mut ",
                "result<",
                "option<",
            ],
        ) + if lower.contains("fn main()") { 3 } else { 0 }
            + if lower.contains("::") { 1 } else { 0 },
    );

    scores.insert(
        "zig",
        count_contains(
            lower,
            &[
                "const std = @import(",
                "@import(",
                "pub fn main(",
                "!void",
                "std.debug.print",
                "comptime",
                "errdefer",
                "[]const u8",
            ],
        ) + if lower.contains("const std = @import(\"std\")") {
            3
        } else {
            0
        },
    );

    scores.insert(
        "go",
        count_contains(
            lower,
            &[
                "package main",
                "func main(",
                "fmt.println",
                "fmt.printf",
                "import (",
                " := ",
                "go func(",
                "defer ",
                "chan ",
                "<-",
            ],
        ),
    );

    scores.insert(
        "python",
        count_contains(
            lower,
            &[
                "def ",
                "class ",
                "import ",
                "from ",
                "print(",
                "lambda ",
                "elif ",
                "__name__ == \"__main__\"",
                "__name__ == '__main__'",
            ],
        ),
    );

    scores.insert(
        "typescript",
        count_contains(
            lower,
            &[
                "interface ",
                "import type ",
                "export type ",
                "readonly ",
                " as const",
                ": string",
                ": number",
                ": boolean",
                "implements ",
                "enum ",
            ],
        ),
    );

    scores.insert(
        "javascript",
        count_contains(
            lower,
            &[
                "function ",
                "console.log(",
                "module.exports",
                "require(",
                "=>",
                "const ",
                "let ",
                "var ",
                "document.",
                "window.",
            ],
        ),
    );

    scores.insert(
        "java",
        count_contains(
            lower,
            &[
                "public class ",
                "public static void main",
                "system.out.println",
                "import java.",
                "package ",
                "private static ",
            ],
        ),
    );

    scores.insert(
        "kotlin",
        count_contains(
            lower,
            &[
                "fun main(",
                "data class ",
                "val ",
                "var ",
                "when (",
                "companion object",
                "println(",
            ],
        ),
    );

    scores.insert(
        "swift",
        count_contains(
            lower,
            &[
                "import foundation",
                "func ",
                "guard let ",
                "if let ",
                "protocol ",
                "print(",
                "let ",
                "var ",
            ],
        ),
    );

    scores.insert(
        "csharp",
        count_contains(
            lower,
            &[
                "using system;",
                "namespace ",
                "console.writeline",
                "static void main",
                "string[] args",
                "get; set;",
                "async task",
                "public class ",
            ],
        ),
    );

    scores.insert(
        "cpp",
        count_contains(
            lower,
            &[
                "#include <iostream>",
                "std::",
                "cout <<",
                "cin >>",
                "namespace std",
                "template<typename",
                "int main(",
            ],
        ),
    );

    scores.insert(
        "c",
        count_contains(
            lower,
            &[
                "#include <stdio.h>",
                "#include <stdlib.h>",
                "printf(",
                "scanf(",
                "malloc(",
                "free(",
                "int main(",
            ],
        ) - if lower.contains("std::") { 3 } else { 0 },
    );

    scores.insert(
        "php",
        count_contains(
            lower,
            &[
                "<?php",
                "$_post",
                "$_get",
                "echo ",
                "function ",
                "namespace ",
                "->",
            ],
        ),
    );

    scores.insert(
        "ruby",
        count_contains(
            lower,
            &[
                "def ",
                "puts ",
                "class ",
                "module ",
                "do |",
                "end\n",
                "require '",
            ],
        ),
    );

    let ts_score = *scores.get("typescript").unwrap_or(&0);
    let js_score = *scores.get("javascript").unwrap_or(&0);
    if ts_score > 0 && js_score > 0 && ts_score >= js_score {
        scores.insert("javascript", js_score - 1);
    }

    let mut best_language = None;
    let mut best_score = 0_i32;

    for language in [
        "zig",
        "rust",
        "go",
        "typescript",
        "javascript",
        "python",
        "java",
        "kotlin",
        "swift",
        "csharp",
        "cpp",
        "c",
        "php",
        "ruby",
    ] {
        let score = *scores.get(language).unwrap_or(&0);
        if score > best_score {
            best_language = Some(language);
            best_score = score;
        }
    }

    (best_score >= 3).then_some(best_language?).or(None)
}

fn looks_like_json(text: &str) -> bool {
    if !(text.starts_with('{') || text.starts_with('[')) {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };

    match value {
        serde_json::Value::Object(map) => !map.is_empty(),
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return false;
            }

            if items.iter().any(|entry| {
                matches!(
                    entry,
                    serde_json::Value::Object(_) | serde_json::Value::Array(_)
                )
            }) {
                return true;
            }

            text.contains('"') && text.contains(',')
        }
        _ => false,
    }
}

fn looks_like_yaml(text: &str) -> bool {
    let trimmed_text = text.trim();
    if trimmed_text.is_empty() || looks_like_json(trimmed_text) || looks_like_toml(trimmed_text) {
        return false;
    }

    let mut pair_lines = 0_usize;
    let mut key_only_lines = 0_usize;
    let mut list_lines = 0_usize;
    let mut indented_lines = 0_usize;
    let mut machine_key_lines = 0_usize;
    let mut noisy_lines = 0_usize;
    let mut has_doc_marker = false;

    for line in text.lines().take(120) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed == "---" || trimmed == "..." {
            has_doc_marker = true;
            continue;
        }

        let indent = line.chars().take_while(|ch| *ch == ' ').count();
        if indent >= 2 {
            indented_lines += 1;
        }

        let body = if let Some(rest) = trimmed.strip_prefix("- ") {
            list_lines += 1;
            rest.trim_start()
        } else {
            trimmed
        };
        if body.is_empty() {
            continue;
        }

        if body.ends_with(':') {
            let key = body.trim_end_matches(':').trim();
            if is_yaml_key_token(key) {
                key_only_lines += 1;
                if looks_machine_yaml_key(key) {
                    machine_key_lines += 1;
                }
                continue;
            }
            noisy_lines += 1;
            continue;
        }

        if let Some((raw_key, raw_value)) = body.split_once(':') {
            let key = raw_key.trim();
            let value = raw_value.trim();
            if !is_yaml_key_token(key) {
                noisy_lines += 1;
                continue;
            }
            if value.is_empty() {
                key_only_lines += 1;
                if looks_machine_yaml_key(key) {
                    machine_key_lines += 1;
                }
                continue;
            }
            if value.contains(';') || value.starts_with("//") {
                noisy_lines += 1;
                continue;
            }

            pair_lines += 1;
            if looks_machine_yaml_key(key) {
                machine_key_lines += 1;
            }
            continue;
        }

        noisy_lines += 1;
        if noisy_lines > 40 {
            break;
        }
    }

    if pair_lines < 2 {
        return false;
    }

    let structural_signal =
        has_doc_marker || list_lines > 0 || indented_lines > 0 || key_only_lines > 0;
    let structured_lines = pair_lines + key_only_lines + list_lines;

    if structural_signal {
        return structured_lines >= 3
            && machine_key_lines >= 1
            && noisy_lines <= structured_lines.saturating_add(1);
    }

    pair_lines >= 4 && machine_key_lines * 2 >= pair_lines && noisy_lines <= 1
}

fn looks_like_toml(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('{') {
        return false;
    }

    let Ok(value) = toml::from_str::<toml::Value>(trimmed) else {
        return false;
    };
    matches!(value, toml::Value::Table(table) if !table.is_empty())
}

fn is_yaml_key_token(raw: &str) -> bool {
    let key = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`');
    if key.is_empty() || key.len() > 80 || key.contains(char::is_whitespace) {
        return false;
    }

    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn looks_machine_yaml_key(raw: &str) -> bool {
    let key = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`');
    key.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.'))
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn combined_search_score(item: &ScoredRecord) -> f32 {
    let base = if item.lexical_score > 0.0 {
        2.0 + (item.lexical_score * 1.2) + (item.semantic_score * 0.3) + (item.neural_score * 0.5)
    } else if item.neural_score > 0.0 || item.semantic_score > 0.0 {
        (item.semantic_score * 0.35) + (item.neural_score * 0.65)
    } else {
        0.0
    };

    // Recency boost: recent items get up to +0.3, decaying with half-life ~30 days.
    let recency = recency_boost(&item.record.created_at);

    base + recency
}

/// Returns a recency boost between 0.0 and 0.3 based on how recently the item
/// was created. Uses exponential decay with a half-life of approximately 30 days
/// (720 hours).
fn recency_boost(created_at: &str) -> f32 {
    const DECAY_HALF_LIFE_HOURS: f64 = 720.0;
    const MAX_BOOST: f32 = 0.3;

    let Ok(timestamp) = DateTime::parse_from_rfc3339(created_at) else {
        return 0.0;
    };

    let age_hours = Utc::now().signed_duration_since(timestamp).num_minutes() as f64 / 60.0;

    if age_hours <= 0.0 {
        return MAX_BOOST;
    }

    MAX_BOOST * (-age_hours / DECAY_HALF_LIFE_HOURS).exp() as f32
}

fn lexical_match_score(record: &ClipboardRecord, query: &str, query_terms: &[String]) -> f32 {
    let content = record.content.to_ascii_lowercase();
    let description = record.description.to_ascii_lowercase();
    let tags: Vec<String> = record
        .tags
        .iter()
        .filter(|tag| !is_bowl_tag(tag))
        .map(|tag| tag.to_ascii_lowercase())
        .collect();
    let mut score = 0.0_f32;

    if content == query {
        score += 2.4;
    }
    if content.starts_with(query) {
        score += 1.5;
    }
    if content.contains(query) {
        score += 1.0;
    }
    if !description.is_empty() && description.contains(query) {
        score += 0.95;
    }
    if tags.iter().any(|tag| tag == query) {
        score += 1.3;
    }
    if tags.iter().any(|tag| tag.contains(query)) {
        score += 0.8;
    }
    if record.item_type.as_str() == query {
        score += 1.0;
    }

    if !query_terms.is_empty() {
        let mut matched_terms = 0_usize;
        for term in query_terms {
            if term.len() < 2 {
                continue;
            }

            if content.contains(term)
                || description.contains(term)
                || tags.iter().any(|tag| tag.contains(term))
            {
                matched_terms += 1;
            }
        }
        score += matched_terms as f32 / query_terms.len() as f32;
    }

    score
}

pub(crate) fn semantic_embedding(content: &str, seed_terms: &[String]) -> Vec<f32> {
    let normalized = content.trim().to_ascii_lowercase();
    let mut terms = semantic_tokenize(&normalized);
    for term in seed_terms {
        terms.extend(semantic_tokenize(term));
    }

    if terms.is_empty() {
        return vec![0.0; SEMANTIC_VECTOR_DIM];
    }

    let mut vector = vec![0.0_f32; SEMANTIC_VECTOR_DIM];
    let mut canonical_terms = Vec::with_capacity(terms.len());

    for term in terms {
        if term.len() < 2 {
            continue;
        }

        let canonical = canonical_semantic_term(&term).to_owned();
        hash_feature_into_vector(&mut vector, "w:", &canonical, 1.0);
        if let Some(stem) = light_stem(&canonical)
            && stem != canonical
        {
            hash_feature_into_vector(&mut vector, "s:", &stem, 0.65);
        }
        canonical_terms.push(canonical);
    }

    for pair in canonical_terms.windows(2) {
        let feature = format!("{}_{}", pair[0], pair[1]);
        hash_feature_into_vector(&mut vector, "b:", &feature, 0.45);
    }

    for trigram in semantic_char_ngrams(&normalized) {
        hash_feature_into_vector(&mut vector, "c:", &trigram, 0.22);
    }

    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }

    vector
}

fn semantic_tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/') {
            current.push(ch.to_ascii_lowercase());
            continue;
        }

        if !current.is_empty() {
            push_semantic_token(&current, &mut tokens);
            current.clear();
        }
    }

    if !current.is_empty() {
        push_semantic_token(&current, &mut tokens);
    }

    tokens
}

fn push_semantic_token(token: &str, output: &mut Vec<String>) {
    if token.len() >= 2 {
        output.push(token.to_owned());
    }

    for part in token.split([':', '/', '-', '_', '.']) {
        if part.len() >= 2 {
            output.push(part.to_owned());
        }
    }
}

fn canonical_semantic_term(term: &str) -> &str {
    match term {
        // Secrets & credentials
        "pass" | "passwd" | "password" | "pwd" | "secret" | "token" | "apikey" | "api_key"
        | "credential" | "credentials" => "secret",

        // Shell & CLI
        "cmd" | "command" | "shell" | "terminal" | "bash" | "zsh" | "fish" | "sh" => "command",

        // URLs & networking
        "link" | "url" | "uri" | "http" | "https" | "website" | "web" => "url",
        "dns" | "nameserver" | "resolve" => "dns",
        "ssh" | "scp" | "sftp" => "ssh",
        "fw" | "firewall" | "iptables" | "nftables" | "ufw" => "firewall",
        "lb" | "loadbalancer" | "load-balancer" => "loadbalancer",

        // Clipboard / snippet
        "snippet" | "snippets" | "clipboard" | "clip" | "copy" | "paste" => "snippet",

        // Languages
        "javascript" | "nodejs" | "node" | "js" => "javascript",
        "typescript" | "ts" | "tsx" => "typescript",
        "python" | "py" => "python",
        "golang" | "go" => "go",
        "rb" | "ruby" => "ruby",
        "rs" | "rust" => "rust",
        "java" | "jvm" | "jdk" => "java",
        "csharp" | "cs" | "dotnet" => "csharp",

        // Data formats
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "xml" | "html" | "htm" => "markup",
        "csv" | "tsv" => "tabular",
        "md" | "markdown" => "markdown",
        "regex" | "regexp" => "regex",

        // Databases
        "postgres" | "postgresql" | "psql" | "sql" | "mysql" | "sqlite" | "mariadb" => "sql",
        "redis" => "redis",
        "mongo" | "mongodb" => "mongodb",

        // Environment
        "env" | "dotenv" | "environment" => "env",

        // Kubernetes core
        "k8s" | "kubernetes" => "kubernetes",
        "deploy" | "deployment" | "deployments" | "rollout" => "deployment",
        "svc" | "service" | "services" => "service",
        "ns" | "namespace" | "namespaces" => "namespace",
        "pod" | "pods" | "po" => "pod",
        "cm" | "configmap" | "configmaps" => "configmap",
        "pv" | "pvc" | "volume" | "volumes" | "persistentvolume" => "volume",
        "ing" | "ingress" => "ingress",
        "hpa" | "autoscale" | "autoscaler" => "autoscaler",
        "sts" | "statefulset" | "statefulsets" => "statefulset",
        "ds" | "daemonset" | "daemonsets" => "daemonset",
        "cj" | "cronjob" | "cronjobs" => "cronjob",
        "sa" | "serviceaccount" => "serviceaccount",
        "netpol" | "networkpolicy" => "networkpolicy",

        // Containers & images
        "container" | "containers" | "ctr" => "container",
        "img" | "image" | "images" => "image",

        // Infrastructure & security
        "cert" | "certificate" | "certificates" | "tls" | "ssl" => "certificate",
        "cfg" | "config" | "configuration" | "conf" => "config",
        "db" | "database" | "databases" => "database",
        "repo" | "repository" | "repositories" => "repository",

        // Cloud & IaC
        "aws" | "amazon" => "aws",
        "gcp" | "gcloud" | "google-cloud" => "gcp",
        "az" | "azure" => "azure",
        "tf" | "terraform" | "hcl" => "terraform",
        "ansible" | "playbook" => "ansible",
        "helm" | "chart" | "charts" => "helm",

        // DevOps concepts
        "ci" | "cd" | "cicd" | "ci/cd" | "pipeline" | "pipelines" => "cicd",
        "prod" | "production" => "production",
        "stg" | "staging" => "staging",
        "dev" | "development" => "development",

        _ => term,
    }
}

fn light_stem(term: &str) -> Option<String> {
    const SUFFIXES: [&str; 16] = [
        "ations", "ation", "ments", "ment", "ingly", "edly", "tion", "ions", "ing", "ers", "ies",
        "ied", "ed", "ly", "es", "s",
    ];

    for suffix in SUFFIXES {
        if term.len() <= suffix.len() + 3 || !term.ends_with(suffix) {
            continue;
        }

        let mut stem = term.to_owned();
        stem.truncate(term.len() - suffix.len());
        if suffix == "ies" {
            stem.push('y');
        }
        return Some(stem);
    }

    None
}

fn semantic_char_ngrams(value: &str) -> Vec<String> {
    let chars: Vec<char> = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(256)
        .collect();

    if chars.len() < 3 {
        return Vec::new();
    }

    chars
        .windows(3)
        .map(|window| window.iter().collect())
        .collect()
}

fn hash_feature_into_vector(vector: &mut [f32], prefix: &str, feature: &str, weight: f32) {
    if feature.is_empty() || vector.is_empty() {
        return;
    }

    let hash = stable_feature_hash(prefix, feature);
    let index = (hash as usize) % vector.len();
    let sign = if hash & (1_u64 << 63) == 0 { 1.0 } else { -1.0 };
    vector[index] += sign * weight;
}

fn stable_feature_hash(prefix: &str, value: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut hash = FNV_OFFSET;
    for byte in prefix.as_bytes().iter().chain(value.as_bytes()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (lhs, rhs) in left.iter().zip(right.iter()) {
        dot += lhs * rhs;
        left_norm += lhs * lhs;
        right_norm += rhs * rhs;
    }

    let norm = (left_norm.sqrt() * right_norm.sqrt()).max(1e-6);
    (dot / norm).clamp(-1.0, 1.0)
}

pub(crate) fn bounded_text_prefix(value: &str, limit: usize) -> &str {
    if value.len() <= limit {
        return value;
    }

    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }

    &value[..end]
}

pub(crate) fn encode_f32_vec_base64(vec: &[f32]) -> String {
    let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
    BASE64.encode(&bytes)
}

#[allow(dead_code)]
pub(crate) fn decode_f32_vec_base64(encoded: &str) -> Option<Vec<f32>> {
    let bytes = BASE64.decode(encoded).ok()?;
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

#[cfg(test)]
pub(crate) fn record_matches_query(record: &ClipboardRecord, query: &str, tag_only: bool) -> bool {
    record_matches_query_indexed(record, query, tag_only, None)
}

fn record_matches_query_indexed(
    record: &ClipboardRecord,
    query: &str,
    tag_only: bool,
    indexed: Option<&IndexedRecord>,
) -> bool {
    if tag_only {
        let tag_terms: Vec<&str> = query
            .split_whitespace()
            .map(|term| term.trim_start_matches(TAG_ONLY_PREFIX).trim())
            .filter(|term| !term.is_empty())
            .collect();

        if tag_terms.is_empty() {
            return true;
        }

        return tag_terms
            .into_iter()
            .all(|term| record_matches_tag(record, term));
    }

    // Use pre-computed lowercase fields from the index when available,
    // falling back to computing them on-the-fly for non-indexed callers.
    let owned_content;
    let owned_description;
    let content: &str = if let Some(idx) = indexed {
        &idx.content_lower
    } else {
        owned_content = record.content.to_lowercase();
        &owned_content
    };
    let description: &str = if let Some(idx) = indexed {
        &idx.description_lower
    } else {
        owned_description = record.description.to_lowercase();
        &owned_description
    };

    if content.contains(query) {
        return true;
    }
    if !description.is_empty() && description.contains(query) {
        return true;
    }

    if record_matches_tag(record, query) {
        return true;
    }

    let query_terms = tokenize_search_terms(query);
    if query_terms.is_empty() {
        return false;
    }

    // Use pre-computed search tokens from the index when available.
    if let Some(idx) = indexed {
        let token_refs: Vec<&str> = idx.search_tokens.iter().map(|s| s.as_str()).collect();
        return query_terms
            .iter()
            .all(|term| term_matches_record(record, term, content, description, &token_refs));
    }

    let fuzzy_search_text = if description.is_empty() {
        bounded_text_prefix(content, SEARCH_FUZZY_TEXT_LIMIT).to_owned()
    } else {
        format!(
            "{} {}",
            bounded_text_prefix(content, SEARCH_FUZZY_TEXT_LIMIT),
            bounded_text_prefix(description, SEARCH_FUZZY_TEXT_LIMIT.min(512))
        )
    };
    let content_terms = tokenize_search_terms(&fuzzy_search_text);
    query_terms
        .iter()
        .all(|term| term_matches_record(record, term, content, description, &content_terms))
}

pub fn render_parameterized_content(
    content: &str,
    parameters: &[ClipboardParameter],
    assignments: &HashMap<String, String>,
) -> Result<String> {
    if parameters.is_empty() {
        return Ok(content.to_owned());
    }

    let mut normalized_assignments = HashMap::new();
    for (key, value) in assignments {
        let normalized_key = key.trim().to_ascii_lowercase();
        if normalized_key.is_empty() {
            continue;
        }
        normalized_assignments.insert(normalized_key, value.trim().to_owned());
    }

    let mut ordered = parameters.to_vec();
    ordered.sort_unstable_by(|left, right| right.target.len().cmp(&left.target.len()));

    let mut output = content.to_owned();
    let mut replacements = Vec::new();
    for (idx, parameter) in ordered.iter().enumerate() {
        if parameter.target.is_empty() {
            continue;
        }

        let key = parameter.name.trim().to_ascii_lowercase();
        let replacement = normalized_assignments
            .get(&key)
            .ok_or_else(|| anyhow!("missing value for parameter '{}'", parameter.name))?;
        if replacement.is_empty() {
            return Err(anyhow!("value for parameter '{}' is empty", parameter.name));
        }
        let mut placeholder = format!("\u{001F}PASTA_PARAM_{idx}\u{001E}");
        while output.contains(&placeholder) {
            placeholder.push('_');
        }
        output = output.replace(&parameter.target, &placeholder);
        replacements.push((placeholder, replacement.to_owned()));
    }

    for (placeholder, replacement) in replacements {
        output = output.replace(&placeholder, &replacement);
    }

    Ok(output)
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
        if is_bowl_tag(raw) {
            continue;
        }
        insert_tag_variants(raw, &mut terms);
    }

    // Merge the UI-facing language detector so `:yaml` (and friends) match items the
    // user sees labeled that way, even when the stricter storage-side detector
    // abstained at insert time.
    if let Some(language) = crate::ui::detect_language(record.item_type, &record.content)
        && let Some(alias) = language.storage_alias()
    {
        terms.insert(alias.to_owned());
        insert_language_aliases(alias, &mut terms);
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
    if normalized.is_empty() || normalized.starts_with(BOWL_TAG_PREFIX) {
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
        "zig" => {
            terms.insert("ziglang".to_owned());
        }
        "cpp" => {
            terms.insert("c++".to_owned());
            terms.insert("cxx".to_owned());
        }
        "c" => {
            terms.insert("clang".to_owned());
        }
        "csharp" => {
            terms.insert("c#".to_owned());
            terms.insert("cs".to_owned());
            terms.insert("dotnet".to_owned());
        }
        "java" => {
            terms.insert("jvm".to_owned());
        }
        "kotlin" => {
            terms.insert("kt".to_owned());
            terms.insert("kts".to_owned());
        }
        "ruby" => {
            terms.insert("rb".to_owned());
        }
        "sql" => {
            terms.insert("db".to_owned());
            terms.insert("database".to_owned());
        }
        "yaml" => {
            terms.insert("yml".to_owned());
        }
        "dockerfile" => {
            terms.insert("docker".to_owned());
        }
        "makefile" => {
            terms.insert("make".to_owned());
            terms.insert("mk".to_owned());
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
    description: &str,
    content_terms: &[&str],
) -> bool {
    if content.contains(query_term)
        || (!description.is_empty() && description.contains(query_term))
        || record_matches_tag(record, query_term)
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_storage(name: &str) -> ClipboardStorage {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the unix epoch")
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!(
            "pasta-storage-{name}-{}-{unique}.db",
            std::process::id()
        ));

        let storage = ClipboardStorage {
            db_path,
            crypto: CryptoBox::ephemeral(),
            semantic_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedding_cache: Arc::new(Mutex::new(HashMap::new())),
            neural_embedder: Arc::new(Mutex::new(None)),
            memory_index: Arc::new(Mutex::new(MemorySearchIndex::default())),
        };
        storage
            .init_schema()
            .expect("test storage schema should initialize");
        storage
            .rebuild_memory_index()
            .expect("test storage index should initialize");
        storage
    }

    #[test]
    fn semantic_similarity_prefers_related_content() {
        let query = semantic_embedding("remove docker container", &[]);
        let related = semantic_embedding(
            "docker rm -f my_service_container",
            &[String::from("command")],
        );
        let unrelated = semantic_embedding("book flights for vacation", &[String::from("text")]);

        let related_score = cosine_similarity(&query, &related);
        let unrelated_score = cosine_similarity(&query, &unrelated);

        assert!(
            related_score > unrelated_score,
            "expected related score ({related_score}) to exceed unrelated score ({unrelated_score})"
        );
    }

    #[test]
    fn semantic_aliases_bridge_secret_terms() {
        let query = semantic_embedding("password reset guide", &[]);
        let alias_match = semantic_embedding(
            "rotate API token and credentials",
            &[String::from("secret")],
        );
        let unrelated = semantic_embedding("frontend animation timing", &[String::from("css")]);

        let alias_score = cosine_similarity(&query, &alias_match);
        let unrelated_score = cosine_similarity(&query, &unrelated);

        assert!(
            alias_score > unrelated_score,
            "expected alias score ({alias_score}) to exceed unrelated score ({unrelated_score})"
        );
    }

    #[test]
    fn base64_content_is_tagged_and_not_auto_secret() {
        let encoded = BASE64.encode("kubectl get pods -A");
        let (item_type, tags) = classify_clipboard_text(&encoded);

        assert_ne!(
            item_type,
            ClipboardItemType::Password,
            "base64 text should not auto-classify as secret"
        );
        assert!(
            tags.iter().any(|tag| tag.eq_ignore_ascii_case("base64")),
            "base64 text should include the base64 tag"
        );
    }

    #[test]
    fn parameterized_content_replaces_named_targets() {
        let content = "SELECT * FROM t WHERE reg_id = '1001' AND status = 'PENDING';";
        let parameters = vec![
            ClipboardParameter {
                name: "reg_id".to_owned(),
                target: "1001".to_owned(),
            },
            ClipboardParameter {
                name: "status_code".to_owned(),
                target: "PENDING".to_owned(),
            },
        ];

        let assignments = HashMap::from([
            ("reg_id".to_owned(), "2002".to_owned()),
            ("status_code".to_owned(), "APPROVED".to_owned()),
        ]);

        let rendered = render_parameterized_content(content, &parameters, &assignments)
            .expect("parameterized render should succeed");
        assert!(rendered.contains("2002"));
        assert!(rendered.contains("APPROVED"));
    }

    #[test]
    fn parameterized_content_requires_all_values() {
        let content = "reg_id=1001";
        let parameters = vec![ClipboardParameter {
            name: "reg_id".to_owned(),
            target: "1001".to_owned(),
        }];
        let assignments = HashMap::new();

        let result = render_parameterized_content(content, &parameters, &assignments);
        assert!(result.is_err());
    }

    #[test]
    fn parameterized_content_does_not_cascade_replacements() {
        let content = "alpha beta";
        let parameters = vec![
            ClipboardParameter {
                name: "first".to_owned(),
                target: "alpha".to_owned(),
            },
            ClipboardParameter {
                name: "second".to_owned(),
                target: "beta".to_owned(),
            },
        ];
        let assignments = HashMap::from([
            ("first".to_owned(), "beta".to_owned()),
            ("second".to_owned(), "gamma".to_owned()),
        ]);

        let rendered = render_parameterized_content(content, &parameters, &assignments)
            .expect("parameterized render should succeed");
        assert_eq!(rendered, "beta gamma");
    }

    #[test]
    fn record_match_includes_description_text() {
        let record = ClipboardRecord {
            id: 1,
            item_type: ClipboardItemType::Command,
            content: "docker rm -f old_container".to_owned(),
            description: "Remove stale container for local dev reset".to_owned(),
            tags: vec!["command".to_owned()],
            parameters: Vec::new(),
            created_at: "2026-03-11T00:00:00Z".to_owned(),
        };

        assert!(record_matches_query(&record, "stale container", false));
    }

    #[test]
    fn record_match_still_finds_exact_term_beyond_fuzzy_window() {
        let mut content = "x".repeat(SEARCH_FUZZY_TEXT_LIMIT + 128);
        content.push_str(" needle-at-the-end");

        let record = ClipboardRecord {
            id: 2,
            item_type: ClipboardItemType::Text,
            content,
            description: String::new(),
            tags: vec!["text".to_owned()],
            parameters: Vec::new(),
            created_at: "2026-03-11T00:00:00Z".to_owned(),
        };

        assert!(record_matches_query(&record, "needle-at-the-end", false));
    }

    #[test]
    fn record_match_uses_stored_language_tag_aliases() {
        let record = ClipboardRecord {
            id: 3,
            item_type: ClipboardItemType::Code,
            content: "fn main() { println!(\"hi\"); }".to_owned(),
            description: String::new(),
            tags: vec!["lang:rust".to_owned()],
            parameters: Vec::new(),
            created_at: "2026-03-11T00:00:00Z".to_owned(),
        };

        assert!(record_matches_query(&record, "rs", true));
    }

    #[test]
    fn tag_search_matches_ui_detected_language_when_stored_tag_missing() {
        // Content the loose UI detector labels YAML but the strict storage
        // detector skips at insert time (no doc marker, few indented lines).
        let record = ClipboardRecord {
            id: 4,
            item_type: ClipboardItemType::Text,
            content: "name: widget\nversion: 1.2.3".to_owned(),
            description: String::new(),
            tags: vec!["text".to_owned()],
            parameters: Vec::new(),
            created_at: "2026-03-11T00:00:00Z".to_owned(),
        };

        assert!(record_matches_query(&record, "yaml", true));
        assert!(record_matches_query(&record, "yml", true));
    }

    #[test]
    fn normalize_search_query_uses_colon_for_tag_mode() {
        assert_eq!(
            parse_search_query(":rs"),
            SearchQuery::TagOnly {
                effective_query: "rs".to_owned()
            }
        );
        assert_eq!(
            parse_search_query(" : lang:rust command "),
            SearchQuery::TagOnly {
                effective_query: "lang:rust command".to_owned()
            }
        );
        assert_eq!(
            parse_search_query("/rs"),
            SearchQuery::Default {
                effective_query: "/rs".to_owned()
            }
        );
    }

    #[test]
    fn parse_search_query_routes_bowl_commands() {
        assert_eq!(
            parse_search_query(":b ops"),
            SearchQuery::Bowl {
                bowl_query: "ops".to_owned()
            }
        );
        assert_eq!(
            parse_search_query(":e ops"),
            SearchQuery::ExportBowl {
                bowl_query: "ops".to_owned()
            }
        );
    }

    #[test]
    fn rehydrate_bowl_export_item_restores_defaults_and_placeholders() {
        let item = BowlExportItem {
            item_type: "command".to_owned(),
            content: "kubectl logs -f deployment/{{deployment}} -n {{namespace}} --tail={{lines}}"
                .to_owned(),
            description: "Stream logs from a deployment".to_owned(),
            tags: vec!["k8s".to_owned()],
            parameters: vec![
                BowlExportParameter {
                    name: "deployment".to_owned(),
                    default_value: String::new(),
                },
                BowlExportParameter {
                    name: "namespace".to_owned(),
                    default_value: "default".to_owned(),
                },
                BowlExportParameter {
                    name: "lines".to_owned(),
                    default_value: "100".to_owned(),
                },
            ],
            hash_embedding: None,
            neural_embedding: None,
            embedding_model: None,
        };

        let (content, parameters) = rehydrate_bowl_export_item(&item);
        assert_eq!(
            content,
            "kubectl logs -f deployment/{{deployment}} -n default --tail=100"
        );
        assert_eq!(
            parameters,
            vec![
                ClipboardParameter {
                    name: "deployment".to_owned(),
                    target: "{{deployment}}".to_owned(),
                },
                ClipboardParameter {
                    name: "namespace".to_owned(),
                    target: "default".to_owned(),
                },
                ClipboardParameter {
                    name: "lines".to_owned(),
                    target: "100".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn tag_search_autocomplete_context_tracks_completed_terms_and_fragment() {
        let context =
            tag_search_autocomplete_context(" :rust command py").expect("expected tag context");
        assert!(context.completed_terms.contains("rust"));
        assert!(context.completed_terms.contains("command"));
        assert_eq!(context.fragment, "py");

        let context =
            tag_search_autocomplete_context(":rust ").expect("expected tag context after space");
        assert!(context.completed_terms.contains("rust"));
        assert_eq!(context.fragment, "");
    }

    #[test]
    fn unmark_item_as_secret_restores_plain_command_content() {
        let storage = test_storage("unmark-secret");
        let content = "kubectl get pods -n default -o wide";

        storage
            .upsert_clipboard_item(content)
            .expect("seed command should insert");
        let inserted = storage
            .search_items("", 10, false, SearchExecution::Fast, 1, None)
            .expect("should load inserted items");
        let item_id = inserted.first().expect("expected inserted item").id;

        assert!(
            storage
                .mark_item_as_secret(item_id)
                .expect("mark secret should succeed")
        );
        assert!(
            storage
                .unmark_item_as_secret(item_id)
                .expect("unmark secret should succeed")
        );

        let restored = storage
            .search_items("", 10, false, SearchExecution::Fast, 2, None)
            .expect("should load restored items")
            .into_iter()
            .find(|item| item.id == item_id)
            .expect("restored item should still exist");

        assert_eq!(restored.item_type, ClipboardItemType::Command);
        assert_eq!(restored.content, content);
        assert!(
            restored
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case("shell"))
        );
        assert!(
            restored.tags.iter().all(|tag| {
                !matches!(
                    tag.to_ascii_lowercase().as_str(),
                    "secret" | "sensitive" | "pass" | "password" | "high_entropy" | "token"
                )
            }),
            "restored tags should not keep secret markers: {:?}",
            restored.tags
        );

        let _ = fs::remove_file(&storage.db_path);
    }

    #[test]
    fn json_detection_requires_structural_json() {
        assert_eq!(
            detect_language_tag(ClipboardItemType::Text, r#"{"id":"101","email":"a@b.com"}"#),
            Some("json")
        );
        assert_ne!(
            detect_language_tag(ClipboardItemType::Text, "[1,2,3]"),
            Some("json")
        );
    }

    #[test]
    fn yaml_detection_is_more_conservative_for_plain_text() {
        let not_yaml = "Error: timed out\nReason: upstream disconnected";
        assert_ne!(
            detect_language_tag(ClipboardItemType::Text, not_yaml),
            Some("yaml")
        );

        let yaml = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: sample-config
data:
  mode: prod
"#;
        assert_eq!(
            detect_language_tag(ClipboardItemType::Text, yaml),
            Some("yaml")
        );
    }

    #[test]
    fn yaml_detection_ignores_clock_style_lines() {
        let schedule = "09:00 standup\n10:30 status";
        assert_ne!(
            detect_language_tag(ClipboardItemType::Text, schedule),
            Some("yaml")
        );
    }

    #[test]
    fn popular_language_detection_handles_rust_zig_go() {
        let rust = r#"
use std::collections::HashMap;
fn main() {
    let mut map = HashMap::new();
    println!("{:?}", map);
}
"#;
        let zig = r#"
const std = @import("std");
pub fn main() !void {
    std.debug.print("hi\n", .{});
}
"#;
        let go = r#"
package main
import "fmt"
func main() {
    fmt.Println("hi")
}
"#;

        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, rust),
            Some("rust")
        );
        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, zig),
            Some("zig")
        );
        assert_eq!(detect_language_tag(ClipboardItemType::Code, go), Some("go"));
    }

    #[test]
    fn popular_language_detection_distinguishes_web_stack() {
        let ts = r#"
interface User { id: string; active: boolean }
const user: User = { id: "1", active: true } as const;
"#;
        let js = r#"
const fs = require("fs");
module.exports = () => console.log(fs.readFileSync("a.txt", "utf8"));
"#;
        let css = "body { margin: 0; display: flex; color: #111; }";
        let html = "<!doctype html><html><body><div>Hello</div></body></html>";

        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, ts),
            Some("typescript")
        );
        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, js),
            Some("javascript")
        );
        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, css),
            Some("css")
        );
        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, html),
            Some("html")
        );
    }

    #[test]
    fn popular_language_detection_handles_system_languages() {
        let csharp = r#"
using System;
namespace Demo {
    public class Program {
        public static void Main(string[] args) {
            Console.WriteLine("hi");
        }
    }
}
"#;
        let cpp = r#"
#include <iostream>
int main() {
    std::cout << "hi" << std::endl;
    return 0;
}
"#;
        let c = r#"
#include <stdio.h>
int main(void) {
    printf("hi\n");
    return 0;
}
"#;

        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, csharp),
            Some("csharp")
        );
        assert_eq!(
            detect_language_tag(ClipboardItemType::Code, cpp),
            Some("cpp")
        );
        assert_eq!(detect_language_tag(ClipboardItemType::Code, c), Some("c"));
    }
}
