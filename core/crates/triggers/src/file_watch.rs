//! File-watch subsystem.
//!
//! Polls `file_watch` triggers on the pulse schedule, compares current
//! filesystem state against the trigger's stored `watch_state`, and enqueues
//! a `trigger_events` row for each created/modified file.
//!
//! Config shape:
//! ```json
//! {
//!   "path": "/abs/path/to/watch",
//!   "events": ["created", "modified"],   // optional, defaults to both
//!   "glob": "*.txt",                      // optional, filename pattern
//!   "poll_every_n_ticks": 1               // optional, default 1 (every pulse tick)
//! }
//! ```
//!
//! State shape (per trigger, in `triggers.watch_state`):
//! ```json
//! { "/abs/path/file.txt": { "mtime": 1700000000, "size": 123 } }
//! ```
//!
//! Payload enqueued per event:
//! ```json
//! { "file_path": "...", "event": "created|modified", "size": N, "mtime": N }
//! ```

use async_trait::async_trait;
use glob::Pattern;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};
use uuid::Uuid;
use velkor_pulse::{PulseSubsystem, SubsystemTickResult};

pub struct FileWatchSubsystem {
    pool: PgPool,
}

impl FileWatchSubsystem {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FileWatchRow {
    id: Uuid,
    config: serde_json::Value,
    watch_state: serde_json::Value,
}

#[async_trait]
impl PulseSubsystem for FileWatchSubsystem {
    fn name(&self) -> &str {
        "file_watch"
    }

    async fn tick(&self) -> anyhow::Result<SubsystemTickResult> {
        let triggers = sqlx::query_as::<_, FileWatchRow>(
            "SELECT id, config, watch_state FROM triggers \
             WHERE kind = 'file_watch' AND is_active = TRUE",
        )
        .fetch_all(&self.pool)
        .await?;

        let checked = triggers.len() as u32;
        let mut processed = 0u32;
        let mut failed = 0u32;

        for t in &triggers {
            match scan_and_enqueue(&self.pool, t).await {
                Ok(n) => processed += n,
                Err(e) => {
                    warn!(trigger_id = %t.id, error = %e, "file_watch scan failed");
                    failed += 1;
                }
            }
        }

        Ok(SubsystemTickResult {
            name: "file_watch".to_string(),
            checked,
            processed,
            failed,
            duration_ms: 0,
            details: None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FileEntry {
    mtime: i64,
    size: u64,
}

fn parse_watch_state(value: &serde_json::Value) -> HashMap<String, FileEntry> {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

async fn scan_and_enqueue(pool: &PgPool, t: &FileWatchRow) -> anyhow::Result<u32> {
    let path = t
        .config
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("file_watch trigger missing config.path"))?;

    let events: Vec<String> = t
        .config
        .get("events")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["created".to_string(), "modified".to_string()]);
    let watch_created = events.iter().any(|e| e == "created");
    let watch_modified = events.iter().any(|e| e == "modified");

    let glob_pattern: Option<Pattern> = t
        .config
        .get("glob")
        .and_then(|v| v.as_str())
        .and_then(|s| Pattern::new(s).ok());

    let previous = parse_watch_state(&t.watch_state);
    let mut current: HashMap<String, FileEntry> = HashMap::new();
    let mut enqueued = 0u32;

    // Scan directory (non-recursive for now; recursive is a trivial extension)
    let dir = Path::new(path);
    if !dir.is_dir() {
        // First tick or path vanished — record empty state and move on
        sqlx::query("UPDATE triggers SET watch_state = '{}'::jsonb WHERE id = $1")
            .bind(t.id)
            .execute(pool)
            .await?;
        return Ok(0);
    }

    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if let Some(ref pat) = glob_pattern
            && !pat.matches(&name)
        {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = meta.len();
        let abs = p.to_string_lossy().to_string();
        let entry = FileEntry { mtime, size };

        // Diff against previous state
        let fire = match previous.get(&abs) {
            None if watch_created => Some("created"),
            Some(prev) if watch_modified && (prev.mtime != mtime || prev.size != size) => {
                Some("modified")
            }
            _ => None,
        };

        if let Some(event_kind) = fire {
            let payload = serde_json::json!({
                "file_path": abs,
                "event": event_kind,
                "size": size,
                "mtime": mtime,
            });
            sqlx::query(
                "INSERT INTO trigger_events (trigger_id, payload) VALUES ($1, $2)",
            )
            .bind(t.id)
            .bind(&payload)
            .execute(pool)
            .await?;
            enqueued += 1;
            debug!(trigger_id = %t.id, file = %abs, event = event_kind, "file_watch enqueued event");
        }

        current.insert(abs, entry);
    }

    // Persist updated state
    let new_state = serde_json::to_value(&current)?;
    sqlx::query("UPDATE triggers SET watch_state = $1 WHERE id = $2")
        .bind(new_state)
        .bind(t.id)
        .execute(pool)
        .await?;

    Ok(enqueued)
}
