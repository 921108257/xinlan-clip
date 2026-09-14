//! SQLite persistence for clipboard history.
//!
//! The pool is owned by the application rather than a plugin so that the file
//! is created and migrated before the first command runs, with no round trip
//! through the webview.
//!
//! Ordering rule used everywhere: pinned first, then most recently used.

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

/// Default cap on non-pinned entries before the oldest are trimmed.
pub const DEFAULT_MAX_ITEMS: i64 = 500;

/// Schema, applied idempotently on every start.
///
/// `content_hash` is unique, which is what turns a repeated copy into an update
/// of the existing row instead of a new one. Pinned rows are excluded from the
/// retention count so a favourite outlives transient copies.
pub const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        content_type TEXT    NOT NULL DEFAULT 'text',
        text         TEXT    NOT NULL,
        content_hash TEXT    NOT NULL UNIQUE,
        created_at   INTEGER NOT NULL,
        last_used_at INTEGER NOT NULL,
        use_count    INTEGER NOT NULL DEFAULT 1,
        pinned       INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_entries_created  ON entries(created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_entries_lastused ON entries(last_used_at DESC);

    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";

/// Open (creating if needed) the history database and apply the schema.
pub async fn connect(path: &std::path::Path) -> Result<SqlitePool, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // WAL keeps the per-second capture poll from blocking the UI's reads.
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    // `execute` runs one statement at a time, so split the script.
    for statement in SCHEMA.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .map_err(|e| format!("schema migration failed: {e}"))?;
    }

    Ok(pool)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub content_type: String,
    pub text: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub use_count: i64,
    pub pinned: bool,
}

impl Entry {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Self {
        Self {
            id: row.get("id"),
            content_type: row.get("content_type"),
            text: row.get("text"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
            use_count: row.get("use_count"),
            pinned: row.get::<i64, _>("pinned") != 0,
        }
    }
}

const SELECT_COLUMNS: &str =
    "id, content_type, text, created_at, last_used_at, use_count, pinned";

/// Insert `text`, or refresh the existing row when the hash already exists.
///
/// # Why this is not a plain "bump on match"
///
/// Capture runs on a timer while the panel is open, so the same text is observed
/// over and over. Counting every observation would inflate `use_count` once per
/// second and permanently mark the clip as freshly used, which would in turn
/// distort both the "used N times" label and the retention order.
///
/// An entry is therefore only treated as *reused* when it is not already the
/// most recently used row — that is, when something else was copied in between.
/// Otherwise the call is a no-op and returns `None`.
pub async fn upsert_entry(
    pool: &SqlitePool,
    text: &str,
    hash: &str,
    now: i64,
) -> Result<Option<Entry>, String> {
    // The entry that polling would otherwise keep re-touching.
    let newest: Option<String> = sqlx::query_scalar(
        "SELECT content_hash FROM entries ORDER BY last_used_at DESC, id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    if newest.as_deref() == Some(hash) {
        return Ok(None);
    }

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM entries WHERE content_hash = ?1")
            .bind(hash)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    if let Some(id) = existing {
        // Re-copied after something else: a genuine reuse.
        sqlx::query(
            "UPDATE entries SET last_used_at = ?1, use_count = use_count + 1 WHERE id = ?2",
        )
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        let row = sqlx::query(&format!("SELECT {SELECT_COLUMNS} FROM entries WHERE id = ?1"))
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(Some(Entry::from_row(&row)));
    }

    let row = sqlx::query(&format!(
        "INSERT INTO entries (content_type, text, content_hash, created_at, last_used_at, use_count, pinned)
         VALUES ('text', ?1, ?2, ?3, ?3, 1, 0)
         RETURNING {SELECT_COLUMNS}"
    ))
    .bind(text)
    .bind(hash)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(Some(Entry::from_row(&row)))
}

/// List entries, newest-used first with pinned entries on top.
///
/// `query` filters with a case-insensitive substring match. Escaped LIKE
/// wildcards are neutralised so searching for `%` behaves literally.
pub async fn list_entries(
    pool: &SqlitePool,
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<Entry>, String> {
    let limit = limit.clamp(1, 1000);

    let rows = match query.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => {
            let pattern = format!("%{}%", escape_like(q));
            sqlx::query(&format!(
                "SELECT {SELECT_COLUMNS} FROM entries
                 WHERE text LIKE ?1 ESCAPE '\\'
                 ORDER BY pinned DESC, last_used_at DESC
                 LIMIT ?2"
            ))
            .bind(pattern)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query(&format!(
                "SELECT {SELECT_COLUMNS} FROM entries
                 ORDER BY pinned DESC, last_used_at DESC
                 LIMIT ?1"
            ))
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(Entry::from_row).collect())
}

/// Fetch one entry by id.
pub async fn get_entry(pool: &SqlitePool, id: i64) -> Result<Option<Entry>, String> {
    let row = sqlx::query(&format!("SELECT {SELECT_COLUMNS} FROM entries WHERE id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.as_ref().map(Entry::from_row))
}

/// Record that an entry was reused from the history list.
pub async fn mark_used(pool: &SqlitePool, id: i64, now: i64) -> Result<(), String> {
    sqlx::query("UPDATE entries SET last_used_at = ?1, use_count = use_count + 1 WHERE id = ?2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a single entry. Returns rows affected.
pub async fn delete_entry(pool: &SqlitePool, id: i64) -> Result<u64, String> {
    let result = sqlx::query("DELETE FROM entries WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

/// Delete the whole history, optionally sparing pinned entries.
pub async fn clear_history(pool: &SqlitePool, keep_pinned: bool) -> Result<u64, String> {
    let sql = if keep_pinned {
        "DELETE FROM entries WHERE pinned = 0"
    } else {
        "DELETE FROM entries"
    };
    let result = sqlx::query(sql).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

/// Toggle the pinned flag. Returns the new state, or `None` if the id is gone.
pub async fn toggle_pin(pool: &SqlitePool, id: i64) -> Result<Option<bool>, String> {
    let row = sqlx::query("SELECT pinned FROM entries WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    let Some(row) = row else { return Ok(None) };
    let next = if row.get::<i64, _>("pinned") != 0 { 0 } else { 1 };

    sqlx::query("UPDATE entries SET pinned = ?1 WHERE id = ?2")
        .bind(next)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(next != 0))
}

/// Drop the oldest non-pinned entries once the cap is exceeded.
///
/// Pinned entries never count towards the cap, so they can always be kept.
pub async fn trim(pool: &SqlitePool, max_items: i64) -> Result<u64, String> {
    let max_items = max_items.max(1);
    let result = sqlx::query(
        "DELETE FROM entries WHERE pinned = 0 AND id NOT IN (
             SELECT id FROM entries WHERE pinned = 0
             ORDER BY last_used_at DESC, id DESC
             LIMIT ?1
         )",
    )
    .bind(max_items)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

/// Total and pinned counts, for the panel footer.
pub async fn counts(pool: &SqlitePool) -> Result<(i64, i64), String> {
    let row = sqlx::query("SELECT COUNT(*) AS total, SUM(pinned) AS pinned FROM entries")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    let total: i64 = row.get("total");
    let pinned: Option<i64> = row.try_get("pinned").unwrap_or(Some(0));
    Ok((total, pinned.unwrap_or(0)))
}

/// Escape `LIKE` metacharacters so a literal search string is matched.
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Current Unix time in seconds. Far enough from zero that `0` stays a
/// usable sentinel for "never".
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1)
}

// ------------------------------------------------------------------ settings

/// Read a setting, falling back to `default` when unset or unparsable.
pub async fn get_setting(pool: &SqlitePool, key: &str, default: &str) -> String {
    let row = sqlx::query("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    row.map(|r| r.get::<String, _>("value"))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Persist a setting, replacing any previous value.
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the history cap, clamped to a sane range.
pub async fn max_items(pool: &SqlitePool) -> i64 {
    get_setting(pool, "max_items", &DEFAULT_MAX_ITEMS.to_string())
        .await
        .parse::<i64>()
        .unwrap_or(DEFAULT_MAX_ITEMS)
        .clamp(10, 10_000)
}

/// Whether selecting an entry should also synthesise Ctrl+V.
pub async fn auto_paste(pool: &SqlitePool) -> bool {
    get_setting(pool, "auto_paste", "true").await == "true"
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// An in-memory database with the production schema applied.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database");
        sqlx::query(
            "CREATE TABLE entries (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT    NOT NULL DEFAULT 'text',
                text         TEXT    NOT NULL,
                content_hash TEXT    NOT NULL UNIQUE,
                created_at   INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL,
                use_count    INTEGER NOT NULL DEFAULT 1,
                pinned       INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .execute(&pool)
        .await
        .expect("schema");
        pool
    }

    async fn insert(pool: &SqlitePool, text: &str, at: i64) -> Entry {
        upsert_entry(pool, text, &format!("h{text}"), at)
            .await
            .expect("insert")
            .expect("new entry")
    }

    #[tokio::test]
    async fn repeat_observation_is_not_a_reuse() {
        let pool = test_pool().await;
        insert(&pool, "hello", 100).await;

        // This is what the one-second capture poll looks like.
        for tick in 101..110 {
            assert!(
                upsert_entry(&pool, "hello", "hhello", tick).await.unwrap().is_none(),
                "an unchanged clipboard should not touch the row"
            );
        }

        let listed = list_entries(&pool, None, 10).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].use_count, 1, "polling must not inflate use_count");
        assert_eq!(listed[0].last_used_at, 100, "and must not restamp the row");
    }

    #[tokio::test]
    async fn recopying_after_something_else_counts_as_reuse() {
        let pool = test_pool().await;
        let first = insert(&pool, "hello", 100).await;
        insert(&pool, "other", 200).await;

        let again = upsert_entry(&pool, "hello", "hhello", 300)
            .await
            .expect("upsert")
            .expect("reuse should be recorded");

        assert_eq!(again.id, first.id, "same row should be reused");
        assert_eq!(again.use_count, 2);
        assert_eq!(again.created_at, 100, "original position is kept");
        assert_eq!(again.last_used_at, 300);
        assert_eq!(list_entries(&pool, None, 10).await.unwrap().len(), 2);

        // It is now the newest row, so it sorts first.
        assert_eq!(list_entries(&pool, None, 10).await.unwrap()[0].text, "hello");
    }

    #[tokio::test]
    async fn pinned_entries_sort_first_then_by_recency() {
        let pool = test_pool().await;
        insert(&pool, "oldest", 100).await;
        let middle = insert(&pool, "middle", 200).await;
        insert(&pool, "newest", 300).await;

        toggle_pin(&pool, middle.id).await.unwrap();

        let listed = list_entries(&pool, None, 10).await.unwrap();
        let texts: Vec<&str> = listed.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["middle", "newest", "oldest"]);
    }

    #[tokio::test]
    async fn search_matches_substrings_case_insensitively() {
        let pool = test_pool().await;
        insert(&pool, "Hello World", 100).await;
        insert(&pool, "goodbye", 200).await;

        let hits = list_entries(&pool, Some("hello"), 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "Hello World");
    }

    #[tokio::test]
    async fn search_treats_wildcards_literally() {
        let pool = test_pool().await;
        insert(&pool, "100% done", 100).await;
        insert(&pool, "no percent here", 200).await;

        // Without escaping, `%` would match everything.
        let hits = list_entries(&pool, Some("100%"), 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "100% done");
    }

    #[tokio::test]
    async fn trim_drops_oldest_unpinned_only() {
        let pool = test_pool().await;
        let keep = insert(&pool, "pinned", 100).await;
        toggle_pin(&pool, keep.id).await.unwrap();
        for index in 0..5 {
            insert(&pool, &format!("item{index}"), 200 + index).await;
        }

        let removed = trim(&pool, 2).await.unwrap();
        assert_eq!(removed, 3, "five unpinned entries trimmed to two");

        let remaining = list_entries(&pool, None, 10).await.unwrap();
        let texts: Vec<&str> = remaining.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["pinned", "item4", "item3"]);
    }

    #[tokio::test]
    async fn clear_history_can_spare_pinned_entries() {
        let pool = test_pool().await;
        let keep = insert(&pool, "pinned", 100).await;
        toggle_pin(&pool, keep.id).await.unwrap();
        insert(&pool, "transient", 200).await;

        assert_eq!(clear_history(&pool, true).await.unwrap(), 1);
        assert_eq!(list_entries(&pool, None, 10).await.unwrap().len(), 1);

        assert_eq!(clear_history(&pool, false).await.unwrap(), 1);
        assert_eq!(list_entries(&pool, None, 10).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn toggle_pin_reports_new_state_and_missing_ids() {
        let pool = test_pool().await;
        let entry = insert(&pool, "entry", 100).await;

        assert_eq!(toggle_pin(&pool, entry.id).await.unwrap(), Some(true));
        assert_eq!(toggle_pin(&pool, entry.id).await.unwrap(), Some(false));
        assert_eq!(toggle_pin(&pool, 9999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn counts_reports_total_and_pinned() {
        let pool = test_pool().await;
        let pinned = insert(&pool, "pinned", 100).await;
        toggle_pin(&pool, pinned.id).await.unwrap();
        insert(&pool, "plain", 200).await;

        assert_eq!(counts(&pool).await.unwrap(), (2, 1));
    }

    #[tokio::test]
    async fn settings_round_trip_with_defaults() {
        let pool = test_pool().await;

        assert_eq!(get_setting(&pool, "missing", "fallback").await, "fallback");
        set_setting(&pool, "max_items", "1000").await.unwrap();
        assert_eq!(max_items(&pool).await, 1000);
        set_setting(&pool, "max_items", "1").await.unwrap();
        assert_eq!(max_items(&pool).await, 10, "clamped to the lower bound");

        assert!(auto_paste(&pool).await, "auto paste defaults to on");
        set_setting(&pool, "auto_paste", "false").await.unwrap();
        assert!(!auto_paste(&pool).await);
    }

    #[tokio::test]
    async fn delete_entry_removes_exactly_one_row() {
        let pool = test_pool().await;
        let entry = insert(&pool, "target", 100).await;
        insert(&pool, "other", 200).await;

        assert_eq!(delete_entry(&pool, entry.id).await.unwrap(), 1);
        assert_eq!(delete_entry(&pool, entry.id).await.unwrap(), 0);
        assert_eq!(list_entries(&pool, None, 10).await.unwrap().len(), 1);
    }
}
