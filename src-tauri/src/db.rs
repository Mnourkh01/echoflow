//! SQLite persistence. One connection guarded by a mutex; the app is
//! single-user and low-concurrency, so this is plenty.

use anyhow::{anyhow, Result};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;

use crate::models::{RecordingResult, RecordingSummary, Settings};
use crate::whisper::Segment;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS recordings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                language TEXT NOT NULL,
                language_confidence REAL NOT NULL DEFAULT 0,
                model TEXT NOT NULL,
                audio_path TEXT NOT NULL,
                sample_rate INTEGER NOT NULL,
                full_text TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS segments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recording_id INTEGER NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
                start_ms INTEGER NOT NULL,
                end_ms INTEGER NOT NULL,
                text TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_segments_recording ON segments(recording_id);
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS usage (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                calls INTEGER NOT NULL DEFAULT 0
            );
            INSERT OR IGNORE INTO usage (id, input_tokens, output_tokens, calls)
                VALUES (1, 0, 0, 0);
            "#,
        )?;
        // Migration: older databases predate the pinned column. ALTER fails
        // harmlessly (duplicate column) once it exists, so ignore the result.
        let _ = conn.execute(
            "ALTER TABLE recordings ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_recording(
        &self,
        created_at: &str,
        duration_ms: i64,
        language: &str,
        language_confidence: f32,
        model: &str,
        audio_path: &str,
        sample_rate: u32,
        full_text: &str,
        segments: &[Segment],
    ) -> Result<i64> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO recordings
                (created_at, duration_ms, language, language_confidence, model, audio_path, sample_rate, full_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                created_at,
                duration_ms,
                language,
                language_confidence,
                model,
                audio_path,
                sample_rate,
                full_text
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO segments (recording_id, start_ms, end_ms, text)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for s in segments {
                stmt.execute(rusqlite::params![id, s.start_ms, s.end_ms, s.text])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn get_recording(&self, id: i64) -> Result<RecordingResult> {
        let conn = self.conn.lock();
        let mut rec = conn.query_row(
            "SELECT id, created_at, duration_ms, language, language_confidence, model, audio_path, full_text, pinned
             FROM recordings WHERE id = ?1",
            [id],
            |row| {
                Ok(RecordingResult {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    duration_ms: row.get(2)?,
                    language: row.get(3)?,
                    language_confidence: row.get(4)?,
                    model: row.get(5)?,
                    audio_path: row.get(6)?,
                    full_text: row.get(7)?,
                    pinned: row.get::<_, i64>(8)? != 0,
                    segments: Vec::new(),
                })
            },
        ).map_err(|e| anyhow!("recording {id} not found: {e}"))?;

        let mut stmt = conn.prepare(
            "SELECT start_ms, end_ms, text FROM segments WHERE recording_id = ?1 ORDER BY start_ms",
        )?;
        let rows = stmt.query_map([id], |row| {
            Ok(Segment {
                start_ms: row.get(0)?,
                end_ms: row.get(1)?,
                text: row.get(2)?,
            })
        })?;
        for r in rows {
            rec.segments.push(r?);
        }
        Ok(rec)
    }

    pub fn list_recordings(&self, query: Option<&str>) -> Result<Vec<RecordingSummary>> {
        let conn = self.conn.lock();
        let mut out = Vec::new();
        let map = |row: &rusqlite::Row| -> rusqlite::Result<RecordingSummary> {
            let full: String = row.get(4)?;
            let preview: String = full.chars().take(140).collect();
            Ok(RecordingSummary {
                id: row.get(0)?,
                created_at: row.get(1)?,
                duration_ms: row.get(2)?,
                language: row.get(3)?,
                preview,
                pinned: row.get::<_, i64>(5)? != 0,
            })
        };
        match query {
            Some(q) if !q.trim().is_empty() => {
                let like = format!("%{}%", q.trim());
                let mut stmt = conn.prepare(
                    "SELECT id, created_at, duration_ms, language, full_text, pinned FROM recordings
                     WHERE full_text LIKE ?1 ORDER BY id DESC",
                )?;
                let rows = stmt.query_map([like], map)?;
                for r in rows {
                    out.push(r?);
                }
            }
            _ => {
                let mut stmt = conn.prepare(
                    "SELECT id, created_at, duration_ms, language, full_text, pinned FROM recordings
                     ORDER BY id DESC",
                )?;
                let rows = stmt.query_map([], map)?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    /// Pin/unpin a recording. Pinned recordings are exempt from retention purge.
    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE recordings SET pinned = ?1 WHERE id = ?2",
            rusqlite::params![pinned as i64, id],
        )?;
        Ok(())
    }

    pub fn delete_recording(&self, id: i64) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let audio_path: Option<String> = conn
            .query_row(
                "SELECT audio_path FROM recordings WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .ok();
        conn.execute("DELETE FROM recordings WHERE id = ?1", [id])?;
        Ok(audio_path)
    }

    /// Delete recordings older than `days` and return their audio paths so the
    /// caller can remove the files from disk. `days <= 0` keeps everything.
    /// Timestamps are RFC3339 UTC, so a string compare is a correct time compare.
    pub fn purge_older_than(&self, days: i64) -> Result<Vec<String>> {
        if days <= 0 {
            return Ok(Vec::new());
        }
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let conn = self.conn.lock();
        let mut paths = Vec::new();
        {
            let mut stmt = conn
                .prepare("SELECT audio_path FROM recordings WHERE created_at < ?1 AND pinned = 0")?;
            let rows = stmt.query_map([&cutoff], |row| row.get::<_, String>(0))?;
            for r in rows {
                paths.push(r?);
            }
        }
        conn.execute(
            "DELETE FROM recordings WHERE created_at < ?1 AND pinned = 0",
            [&cutoff],
        )?;
        Ok(paths)
    }

    /// Delete every recording (segments cascade) and return all audio paths so
    /// the caller can remove the files. Permanent: this is the "clear history" op.
    pub fn clear_all_recordings(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut paths = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT audio_path FROM recordings")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for r in rows {
                paths.push(r?);
            }
        }
        conn.execute("DELETE FROM recordings", [])?;
        Ok(paths)
    }

    pub fn add_usage(&self, input: i64, output: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE usage SET input_tokens = input_tokens + ?1,
                              output_tokens = output_tokens + ?2,
                              calls = calls + 1
             WHERE id = 1",
            rusqlite::params![input, output],
        )?;
        Ok(())
    }

    pub fn get_usage(&self) -> Result<(i64, i64, i64)> {
        let conn = self.conn.lock();
        let row = conn.query_row(
            "SELECT input_tokens, output_tokens, calls FROM usage WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(row)
    }

    pub fn reset_usage(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE usage SET input_tokens = 0, output_tokens = 0, calls = 0 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Settings {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'app'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [json],
        )?;
        Ok(())
    }
}
