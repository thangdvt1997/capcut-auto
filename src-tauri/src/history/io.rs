//! Real SQLite persistence for [`super::HistoryEntry`] — schema + CRUD
//! against the *same* `rusqlite::Connection`/file `db::MediaLibrary` already
//! manages (see `history` module doc comment for the full storage-location
//! writeup). Every function here takes a plain `&Connection`, matching
//! `db::mod`'s own house convention exactly (`db::upsert_media`/
//! `db::search_media` etc.) — no `AppHandle` anywhere in this file, so it's
//! directly unit-testable against `Connection::open_in_memory()`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::batch::{BatchJobStatus, BatchPipelineConfig};

use super::error::HistoryError;
use super::HistoryEntry;

fn to_db_error(context: &str, err: rusqlite::Error) -> HistoryError {
    HistoryError::DatabaseError {
        details: format!("{context}: {err}"),
    }
}

/// Creates the `history` table if it doesn't already exist. Called once at
/// startup (`lib.rs`'s `run()` setup, right after `commands::media::init_media_library`
/// opens the shared connection) against that same connection — and by every
/// test in this file via `Connection::open_in_memory()`.
pub fn init_schema(conn: &Connection) -> Result<(), HistoryError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
            id                TEXT PRIMARY KEY,
            batch_id          TEXT NOT NULL,
            job_name          TEXT NOT NULL,
            input_path        TEXT NOT NULL,
            output_path       TEXT,
            template_id       TEXT,
            template_version  INTEGER,
            ai_prompt         TEXT,
            ai_result         TEXT,
            execution_plan    TEXT NOT NULL,
            capcut_draft_path TEXT,
            started_at        TEXT NOT NULL,
            ended_at          TEXT,
            duration_us       INTEGER,
            status            TEXT NOT NULL,
            error             TEXT,
            retry_count       INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_history_started_at ON history(started_at);
         CREATE INDEX IF NOT EXISTS idx_history_batch_id ON history(batch_id);",
    )
    .map_err(|e| to_db_error("creating history schema", e))
}

/// In-memory database, for unit tests — mirrors `db::open_in_memory`'s own
/// doc comment/rationale exactly (avoids touching the filesystem, lets tests
/// run in parallel without colliding on a shared file).
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, HistoryError> {
    let conn = Connection::open_in_memory().map_err(|e| to_db_error("opening in-memory db", e))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn status_to_str(status: BatchJobStatus) -> &'static str {
    match status {
        BatchJobStatus::Queued => "queued",
        BatchJobStatus::Analyzing => "analyzing",
        BatchJobStatus::Transcribing => "transcribing",
        BatchJobStatus::Editing => "editing",
        BatchJobStatus::Rendering => "rendering",
        BatchJobStatus::Paused => "paused",
        BatchJobStatus::Completed => "completed",
        BatchJobStatus::Failed => "failed",
        BatchJobStatus::Cancelled => "cancelled",
    }
}

fn status_from_str(s: &str) -> BatchJobStatus {
    match s {
        "queued" => BatchJobStatus::Queued,
        "analyzing" => BatchJobStatus::Analyzing,
        "transcribing" => BatchJobStatus::Transcribing,
        "editing" => BatchJobStatus::Editing,
        "rendering" => BatchJobStatus::Rendering,
        "paused" => BatchJobStatus::Paused,
        "completed" => BatchJobStatus::Completed,
        "failed" => BatchJobStatus::Failed,
        _ => BatchJobStatus::Cancelled,
    }
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    let execution_plan_json: String = row.get("execution_plan")?;
    let execution_plan: BatchPipelineConfig =
        serde_json::from_str(&execution_plan_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let status_str: String = row.get("status")?;
    Ok(HistoryEntry {
        id: row.get("id")?,
        batch_id: row.get("batch_id")?,
        job_name: row.get("job_name")?,
        input_path: row.get("input_path")?,
        output_path: row.get("output_path")?,
        template_id: row.get("template_id")?,
        template_version: row.get("template_version")?,
        ai_prompt: row.get("ai_prompt")?,
        ai_result: row.get("ai_result")?,
        execution_plan,
        capcut_draft_path: row.get("capcut_draft_path")?,
        started_at: row.get("started_at")?,
        ended_at: row.get("ended_at")?,
        duration_us: row.get("duration_us")?,
        status: status_from_str(&status_str),
        error: row.get("error")?,
        retry_count: row.get("retry_count")?,
    })
}

/// Inserts a fresh row for `entry.id`, or — if a row with that `id` already
/// exists (i.e. this is the same logical batch job reaching a terminal state
/// again after a retry, `batch::manager`'s own "same `job_id`, reset to
/// `Queued`" retry semantics) — updates it in place and atomically bumps
/// `retry_count` by 1 (`ON CONFLICT ... retry_count = history.retry_count +
/// 1`, done inside SQLite itself rather than a separate read-then-write, so
/// there's no read/write race between two calls). `entry.retry_count` itself
/// is never trusted/written directly — the returned, re-read `HistoryEntry`
/// always carries the real, database-computed count. This is the one
/// function `batch::manager` calls every time a job reaches
/// `Completed`/`Failed`/`Cancelled`.
pub fn record_terminal(
    conn: &Connection,
    entry: &HistoryEntry,
) -> Result<HistoryEntry, HistoryError> {
    let execution_plan_json =
        serde_json::to_string(&entry.execution_plan).map_err(|e| HistoryError::DatabaseError {
            details: format!("serializing execution_plan: {e}"),
        })?;
    conn.execute(
        "INSERT INTO history (id, batch_id, job_name, input_path, output_path, template_id, template_version, ai_prompt, ai_result, execution_plan, capcut_draft_path, started_at, ended_at, duration_us, status, error, retry_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0)
         ON CONFLICT(id) DO UPDATE SET
            batch_id = excluded.batch_id,
            job_name = excluded.job_name,
            input_path = excluded.input_path,
            output_path = excluded.output_path,
            template_id = excluded.template_id,
            template_version = excluded.template_version,
            ai_prompt = excluded.ai_prompt,
            ai_result = excluded.ai_result,
            execution_plan = excluded.execution_plan,
            capcut_draft_path = excluded.capcut_draft_path,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            duration_us = excluded.duration_us,
            status = excluded.status,
            error = excluded.error,
            retry_count = history.retry_count + 1",
        params![
            entry.id,
            entry.batch_id,
            entry.job_name,
            entry.input_path,
            entry.output_path,
            entry.template_id,
            entry.template_version,
            entry.ai_prompt,
            entry.ai_result,
            execution_plan_json,
            entry.capcut_draft_path,
            entry.started_at,
            entry.ended_at,
            entry.duration_us,
            status_to_str(entry.status),
            entry.error,
        ],
    )
    .map_err(|e| to_db_error("recording history terminal state", e))?;

    get_history_entry(conn, &entry.id)?.ok_or_else(|| HistoryError::DatabaseError {
        details: format!("history row {} missing immediately after write", entry.id),
    })
}

pub fn get_history_entry(
    conn: &Connection,
    id: &str,
) -> Result<Option<HistoryEntry>, HistoryError> {
    conn.query_row(
        "SELECT * FROM history WHERE id = ?1",
        params![id],
        row_to_entry,
    )
    .optional()
    .map_err(|e| to_db_error("get_history_entry", e))
}

/// Newest-first (by `started_at`), real `LIMIT`/`OFFSET` pagination — same
/// query-building convention `db::search_media` already established
/// (bounded `LIMIT`, no unbounded "give me everything" query path).
pub fn list_history(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistoryEntry>, HistoryError> {
    let mut stmt = conn
        .prepare("SELECT * FROM history ORDER BY started_at DESC LIMIT ?1 OFFSET ?2")
        .map_err(|e| to_db_error("preparing list_history", e))?;
    let rows = stmt
        .query_map(params![limit, offset], row_to_entry)
        .map_err(|e| to_db_error("running list_history", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_db_error("reading list_history rows", e))
}

/// Real row count, for a frontend that wants "page N of M" — not itself
/// required by any command yet, but a one-line query worth having alongside
/// `list_history`'s own pagination rather than making a future caller derive
/// it by fetching every page.
pub fn count_history(conn: &Connection) -> Result<u32, HistoryError> {
    conn.query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
        .map_err(|e| to_db_error("count_history", e))
}

/// Deletes one row — the real primitive behind a possible future "clear
/// history" / "delete this run" frontend action (§21 doesn't spell out a
/// delete affordance explicitly, but a history table with no way to prune it
/// is an odd real gap; kept minimal — no bulk "delete all" here, a caller can
/// loop `list_history` + this for that if ever needed).
pub fn delete_history_entry(conn: &Connection, id: &str) -> Result<(), HistoryError> {
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])
        .map_err(|e| to_db_error("delete_history_entry", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vad::CutParams;

    fn config() -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: Some(CutParams::default()),
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: Some("tmpl_tiktok".to_string()),
            export_preset_id: Some("p1080".to_string()),
            output_suffix: Some("tiktok".to_string()),
        }
    }

    fn entry(id: &str, status: BatchJobStatus) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            batch_id: "batch1".to_string(),
            job_name: format!("{id}.mp4"),
            input_path: format!("/media/{id}.mp4"),
            output_path: Some(format!("/media/batch_output/{id}_tiktok.mp4")),
            template_id: Some("tmpl_tiktok".to_string()),
            template_version: Some(1),
            ai_prompt: None,
            ai_result: None,
            execution_plan: config(),
            capcut_draft_path: None,
            started_at: "2026-09-06T00:00:00Z".to_string(),
            ended_at: Some("2026-09-06T00:01:00Z".to_string()),
            duration_us: Some(60_000_000),
            status,
            error: None,
            retry_count: 0,
        }
    }

    #[test]
    fn round_trips_a_history_entry() {
        let conn = open_in_memory().unwrap();
        let e = entry("job1", BatchJobStatus::Completed);
        record_terminal(&conn, &e).unwrap();

        let found = get_history_entry(&conn, "job1").unwrap().unwrap();
        assert_eq!(found.id, "job1");
        assert_eq!(found.status, BatchJobStatus::Completed);
        assert_eq!(found.execution_plan, e.execution_plan);
        assert_eq!(found.output_path, e.output_path);
        assert_eq!(found.retry_count, 0);
    }

    #[test]
    fn get_history_entry_returns_none_for_an_unknown_id() {
        let conn = open_in_memory().unwrap();
        assert!(get_history_entry(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn recording_the_same_job_id_again_increments_retry_count_instead_of_duplicating() {
        let conn = open_in_memory().unwrap();
        let mut e = entry("job1", BatchJobStatus::Failed);
        e.error = Some("media file not found".to_string());
        record_terminal(&conn, &e).unwrap();

        // The job was retried and this time completed — same id, fresh
        // outcome.
        let mut retried = entry("job1", BatchJobStatus::Completed);
        retried.error = None;
        let after_retry = record_terminal(&conn, &retried).unwrap();

        assert_eq!(after_retry.retry_count, 1, "one retry so far");
        assert_eq!(after_retry.status, BatchJobStatus::Completed);
        assert!(after_retry.error.is_none());

        // Still exactly one row, not two.
        assert_eq!(list_history(&conn, 100, 0).unwrap().len(), 1);
        assert_eq!(count_history(&conn).unwrap(), 1);

        // A second retry bumps it again.
        let again = record_terminal(&conn, &entry("job1", BatchJobStatus::Completed)).unwrap();
        assert_eq!(again.retry_count, 2);
        assert_eq!(list_history(&conn, 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn a_different_job_id_creates_a_genuinely_new_row() {
        let conn = open_in_memory().unwrap();
        record_terminal(&conn, &entry("job1", BatchJobStatus::Completed)).unwrap();
        record_terminal(&conn, &entry("job2", BatchJobStatus::Completed)).unwrap();
        assert_eq!(list_history(&conn, 100, 0).unwrap().len(), 2);
    }

    #[test]
    fn list_history_orders_newest_started_first() {
        let conn = open_in_memory().unwrap();
        let mut e1 = entry("job1", BatchJobStatus::Completed);
        e1.started_at = "2026-09-01T00:00:00Z".to_string();
        let mut e2 = entry("job2", BatchJobStatus::Completed);
        e2.started_at = "2026-09-05T00:00:00Z".to_string();
        record_terminal(&conn, &e1).unwrap();
        record_terminal(&conn, &e2).unwrap();

        let listed = list_history(&conn, 100, 0).unwrap();
        assert_eq!(listed[0].id, "job2", "the more recent job comes first");
        assert_eq!(listed[1].id, "job1");
    }

    #[test]
    fn list_history_respects_limit_and_offset_real_pagination() {
        let conn = open_in_memory().unwrap();
        for i in 0..5 {
            let mut e = entry(&format!("job{i}"), BatchJobStatus::Completed);
            e.started_at = format!("2026-09-0{}T00:00:00Z", i + 1);
            record_terminal(&conn, &e).unwrap();
        }
        let page1 = list_history(&conn, 2, 0).unwrap();
        let page2 = list_history(&conn, 2, 2).unwrap();
        let page3 = list_history(&conn, 2, 4).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        assert_eq!(page3.len(), 1);
        // Newest-first, 5 total: job4, job3, job2, job1, job0.
        assert_eq!(page1[0].id, "job4");
        assert_eq!(page1[1].id, "job3");
        assert_eq!(page2[0].id, "job2");
        assert_eq!(page3[0].id, "job0");
    }

    #[test]
    fn delete_history_entry_removes_the_row() {
        let conn = open_in_memory().unwrap();
        record_terminal(&conn, &entry("job1", BatchJobStatus::Completed)).unwrap();
        delete_history_entry(&conn, "job1").unwrap();
        assert!(get_history_entry(&conn, "job1").unwrap().is_none());
        assert_eq!(count_history(&conn).unwrap(), 0);
    }

    #[test]
    fn a_failed_job_records_its_real_error_and_no_output_path() {
        let conn = open_in_memory().unwrap();
        let mut e = entry("job1", BatchJobStatus::Failed);
        e.output_path = None;
        e.error = Some("ffmpeg exited with a non-zero status".to_string());
        record_terminal(&conn, &e).unwrap();

        let found = get_history_entry(&conn, "job1").unwrap().unwrap();
        assert_eq!(found.status, BatchJobStatus::Failed);
        assert!(found.output_path.is_none());
        assert_eq!(
            found.error.as_deref(),
            Some("ffmpeg exited with a non-zero status")
        );
    }

    #[test]
    fn a_cancelled_job_records_correctly() {
        let conn = open_in_memory().unwrap();
        let mut e = entry("job1", BatchJobStatus::Cancelled);
        e.output_path = None;
        record_terminal(&conn, &e).unwrap();
        let found = get_history_entry(&conn, "job1").unwrap().unwrap();
        assert_eq!(found.status, BatchJobStatus::Cancelled);
    }

    #[test]
    fn template_version_and_ai_fields_round_trip_including_none() {
        let conn = open_in_memory().unwrap();
        let mut e = entry("job1", BatchJobStatus::Completed);
        e.template_id = None;
        e.template_version = None;
        e.ai_prompt = None;
        e.ai_result = None;
        e.capcut_draft_path = None;
        record_terminal(&conn, &e).unwrap();

        let found = get_history_entry(&conn, "job1").unwrap().unwrap();
        assert!(found.template_id.is_none());
        assert!(found.template_version.is_none());
        assert!(found.ai_prompt.is_none());
        assert!(found.ai_result.is_none());
        assert!(found.capcut_draft_path.is_none());
    }

    #[test]
    fn every_batch_job_status_round_trips_through_the_str_mapping() {
        let conn = open_in_memory().unwrap();
        let all = [
            BatchJobStatus::Queued,
            BatchJobStatus::Analyzing,
            BatchJobStatus::Transcribing,
            BatchJobStatus::Editing,
            BatchJobStatus::Rendering,
            BatchJobStatus::Paused,
            BatchJobStatus::Completed,
            BatchJobStatus::Failed,
            BatchJobStatus::Cancelled,
        ];
        for (i, status) in all.iter().enumerate() {
            let e = entry(&format!("job{i}"), *status);
            record_terminal(&conn, &e).unwrap();
            let found = get_history_entry(&conn, &format!("job{i}"))
                .unwrap()
                .unwrap();
            assert_eq!(found.status, *status);
        }
    }
}
