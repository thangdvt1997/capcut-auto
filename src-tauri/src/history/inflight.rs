//! `in_progress_jobs` — real, additive persistence backing **crash-recovery
//! *detection*** for batch jobs (`STUDIO_PLAN.md` Phase D8a, `promt.md` §17
//! "JOB STATE / RESUME"). See this file's own doc comment for the honest
//! scope line, repeated here because it's the single most important thing
//! about this table: **this is detection, not resume.**
//!
//! `batch::manager::JobState` (that module's own doc comment) is in-memory
//! only — a `Arc<Mutex<JobState>>` per job, gone the instant the process
//! exits, whether that exit is clean or a crash. If the app is killed while
//! a batch job is still mid-flight (`Analyzing`/`Transcribing`/`Editing`/
//! `Rendering`/`Paused`, or even still `Queued`), that job today simply
//! vanishes — no trace anywhere that it ever existed, `history`'s own
//! `history` table included (a row there is only ever written once a job
//! reaches a *terminal* status — see `history` module doc comment). This
//! table exists to close exactly that one honesty gap, and no further.
//!
//! ## What this is NOT: promt.md §17's own worked example
//!
//! §17's own mockup describes true resume: "video đã Extract✓ Translate✓
//! Voice✓ Render✗ … restart app: resume từ Render" — i.e. skip every
//! already-completed stage and continue from the failed one, reusing that
//! stage's already-computed output. Building that for real would require
//! restructuring how `batch::pipeline::run_pipeline` hands each stage's
//! output to the next: today it is one single, monolithic, run-to-completion
//! function with no concept of "start at stage N using stage N-1's already-
//! computed artifact" — the in-progress `ProjectV1` being edited lives only
//! in a stack-local variable inside that function, never persisted anywhere
//! a restart could pick back up from (`batch::manager` module doc comment's
//! own "Retry semantics" section already documents this exact same gap for
//! plain in-process retry, which restarts from scratch for the identical
//! reason). Restructuring that is a large, separate, high-risk architecture
//! project on the single most complex subsystem in this codebase (freshly
//! rearchitected for the Phase D4a N-worker pool) — explicitly out of scope
//! here. Nothing in `run_pipeline`'s stage execution logic, its stage-to-
//! stage data flow, or `spawn_worker_pool`'s concurrency machinery is touched
//! by this module.
//!
//! ## What this genuinely is: honest detection + surfacing
//!
//! One row per **currently non-terminal** batch job — written/updated by
//! `batch::manager::record_inflight_for_job` every time that job settles
//! into a new non-terminal [`BatchJobStatus`] (`Queued`/`Analyzing`/
//! `Transcribing`/`Editing`/`Rendering`/`Paused`), and deleted the instant it
//! reaches ANY terminal status (`Completed`/`Failed`/`Cancelled`) — see that
//! function's own doc comment for exactly which real state-transition points
//! call it. **By construction, any row still present in this table the next
//! time the app starts is an orphaned job**: the process ended before that
//! job's own worker thread ever reached the one code path that would have
//! cleared its row.
//!
//! [`recover_orphaned_jobs`] is the startup-time consumer (`lib.rs`'s
//! `run()` setup, immediately after [`super::io::init_schema`]): read every
//! surviving row, record each one as a real `Failed` [`super::HistoryEntry`]
//! in the existing `history` table with a clear, honest
//! [`INTERRUPTED_ERROR_MESSAGE`] (so it appears in the existing History
//! dialog exactly like any other failed job, retryable via the existing,
//! unchanged Retry mechanism — restarting *from scratch*, never resuming),
//! then clear the row. This is the same "was the last exit clean" question
//! `crate::logging`'s own `.session-active` marker already answers at the
//! whole-app level (`crate::logging` module doc comment, master prompt §86)
//! — this table answers the same question per in-flight *batch job*, with
//! enough real data (input path, full `BatchPipelineConfig`) to make the
//! resulting `Failed` history row actually re-runnable via Retry, not merely
//! informative.
//!
//! Lives in the same `history` module / same SQLite connection / same
//! database file the `history` table itself already uses (`history` module
//! doc comment's own storage-location decision, followed here unchanged) —
//! not a new database, not a new connection.

use rusqlite::{params, Connection};

use crate::batch::{BatchJobStatus, BatchPipelineConfig};

use super::error::HistoryError;
use super::io::{status_from_str, status_to_str};
use super::HistoryEntry;

/// The exact, honest error message a recovered orphaned job's `history` row
/// carries — deliberately says *why* (an app restart, not a real pipeline
/// failure) and *what to do* (Retry re-runs it from scratch, exactly like
/// any other failed job today).
pub const INTERRUPTED_ERROR_MESSAGE: &str =
    "Interrupted by app restart — click Retry to reprocess from the start.";

fn to_db_error(context: &str, err: rusqlite::Error) -> HistoryError {
    HistoryError::DatabaseError {
        details: format!("{context}: {err}"),
    }
}

/// One still-non-terminal batch job, as tracked by `batch::manager` at the
/// moment this row was last written — everything needed to (a) recognize it
/// as orphaned at the next startup and (b) build a real, re-runnable
/// `history::HistoryEntry` for it. Not specta-typed/exposed to the frontend
/// — this table is purely an internal recovery mechanism; the frontend only
/// ever sees the resulting `Failed` `history` row.
#[derive(Debug, Clone, PartialEq)]
pub struct InFlightJob {
    /// Exactly the originating `BatchJob::id` (`batch::manager::JobState::id`)
    /// — same convention `HistoryEntry::id` already uses.
    pub job_id: String,
    pub batch_id: String,
    pub job_name: String,
    pub input_path: String,
    /// The real config this job would need to be re-run from scratch (this
    /// is detection + honest surfacing, not resume — see module doc
    /// comment) — carried through unchanged into the recovered
    /// `HistoryEntry::execution_plan`, so Retry/re-run against it behaves
    /// exactly like any other history row.
    pub config: BatchPipelineConfig,
    pub status: BatchJobStatus,
    pub stage: String,
    /// RFC3339 — the job's real `JobState::started_at_rfc3339` at the moment
    /// this row was last written.
    pub started_at: String,
}

/// Creates the `in_progress_jobs` table if it doesn't already exist. Called
/// once at startup (`lib.rs`'s `run()` setup, immediately after
/// `super::io::init_schema` against that same connection), and by every test
/// in this file via `super::io::open_in_memory`-style in-memory connections.
pub fn init_schema(conn: &Connection) -> Result<(), HistoryError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS in_progress_jobs (
            job_id     TEXT PRIMARY KEY,
            batch_id   TEXT NOT NULL,
            job_name   TEXT NOT NULL,
            input_path TEXT NOT NULL,
            config     TEXT NOT NULL,
            status     TEXT NOT NULL,
            stage      TEXT NOT NULL,
            started_at TEXT NOT NULL
         );",
    )
    .map_err(|e| to_db_error("creating in_progress_jobs schema", e))
}

/// In-memory database with both this table and `history`'s own schema
/// created — mirrors `super::io::open_in_memory`'s own doc comment/rationale
/// (avoids touching the filesystem, lets tests run in parallel without
/// colliding on a shared file). Used by this file's own tests, which
/// exercise [`recover_orphaned_jobs`] writing real rows into the *same*
/// `history` table `super::io`'s own tests already cover.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, HistoryError> {
    let conn = super::io::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

fn row_to_inflight(row: &rusqlite::Row) -> rusqlite::Result<InFlightJob> {
    let config_json: String = row.get("config")?;
    let config: BatchPipelineConfig = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status_str: String = row.get("status")?;
    Ok(InFlightJob {
        job_id: row.get("job_id")?,
        batch_id: row.get("batch_id")?,
        job_name: row.get("job_name")?,
        input_path: row.get("input_path")?,
        config,
        status: status_from_str(&status_str),
        stage: row.get("stage")?,
        started_at: row.get("started_at")?,
    })
}

/// Writes (or overwrites) `job`'s row — the real "record/update on every
/// non-terminal status transition" half of this module's design
/// (`batch::manager::record_inflight_for_job`'s only caller). A plain
/// `INSERT OR REPLACE` — unlike `history::io::record_terminal`'s own
/// `ON CONFLICT ... retry_count = retry_count + 1` upsert, there is no
/// auxiliary counter here worth preserving across an overwrite: this row's
/// entire point is "the most recently known non-terminal state," nothing
/// about its own history matters once it's superseded.
pub fn upsert_inflight(conn: &Connection, job: &InFlightJob) -> Result<(), HistoryError> {
    let config_json =
        serde_json::to_string(&job.config).map_err(|e| HistoryError::DatabaseError {
            details: format!("serializing in-flight job config: {e}"),
        })?;
    conn.execute(
        "INSERT OR REPLACE INTO in_progress_jobs
            (job_id, batch_id, job_name, input_path, config, status, stage, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            job.job_id,
            job.batch_id,
            job.job_name,
            job.input_path,
            config_json,
            status_to_str(job.status),
            job.stage,
            job.started_at,
        ],
    )
    .map_err(|e| to_db_error("upserting in-flight job", e))?;
    Ok(())
}

/// Deletes one row — called the instant a job reaches ANY terminal status
/// (`batch::manager::record_inflight_for_job`). Deleting a row that doesn't
/// exist (e.g. a job that failed before its first non-terminal write ever
/// landed) is not an error — `DELETE ... WHERE` matching zero rows is a
/// normal, silent no-op in SQLite.
pub fn clear_inflight(conn: &Connection, job_id: &str) -> Result<(), HistoryError> {
    conn.execute(
        "DELETE FROM in_progress_jobs WHERE job_id = ?1",
        params![job_id],
    )
    .map_err(|e| to_db_error("clearing in-flight job", e))?;
    Ok(())
}

/// Every surviving row, oldest-`started_at`-first (a deterministic, testable
/// order — nothing downstream depends on any particular ordering, since
/// every row here is, by construction, about to be recovered and cleared).
pub fn list_inflight(conn: &Connection) -> Result<Vec<InFlightJob>, HistoryError> {
    let mut stmt = conn
        .prepare("SELECT * FROM in_progress_jobs ORDER BY started_at ASC")
        .map_err(|e| to_db_error("preparing list_inflight", e))?;
    let rows = stmt
        .query_map([], row_to_inflight)
        .map_err(|e| to_db_error("running list_inflight", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_db_error("reading list_inflight rows", e))
}

/// A real, best-effort elapsed-time estimate for a recovered job — the time
/// between its last-known `started_at` and this recovery scan running "now."
/// Falls back to `None` (never a fabricated number) if `started_at` somehow
/// isn't valid RFC3339 — defensive only; every real writer of this table
/// (`batch::manager::build_inflight_job`) always uses
/// `crate::project::now_rfc3339()`, which always produces one.
fn duration_since_started(started_at: &str) -> Option<i64> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let now = chrono::Utc::now();
    now.signed_duration_since(started.with_timezone(&chrono::Utc))
        .num_microseconds()
}

/// The real startup-time recovery scan (module doc comment): reads every
/// surviving row, records each as a real `Failed` [`HistoryEntry`] (this
/// pass's honest [`INTERRUPTED_ERROR_MESSAGE`], `execution_plan` carried
/// through unchanged so Retry/re-run behaves normally), then clears the row.
/// Returns how many jobs were recovered — `lib.rs`'s own call site logs this
/// count (0 is the overwhelmingly common case: a clean previous exit leaves
/// this table empty).
///
/// `template_version` is honestly `None` here (this scan has no
/// `templates_dir`/running app to re-resolve it against) — the exact same
/// "no templates_dir available" case `batch::manager::build_history_entry`
/// already produces on its own early-failure path, not a new kind of gap
/// this module introduces.
pub fn recover_orphaned_jobs(conn: &Connection) -> Result<usize, HistoryError> {
    let orphans = list_inflight(conn)?;
    let now = crate::project::now_rfc3339();
    for orphan in &orphans {
        let entry = HistoryEntry {
            id: orphan.job_id.clone(),
            batch_id: orphan.batch_id.clone(),
            job_name: orphan.job_name.clone(),
            input_path: orphan.input_path.clone(),
            output_path: None,
            template_id: orphan.config.template_id.clone(),
            template_version: None,
            ai_prompt: None,
            ai_result: None,
            execution_plan: orphan.config.clone(),
            capcut_draft_path: None,
            started_at: orphan.started_at.clone(),
            ended_at: Some(now.clone()),
            duration_us: duration_since_started(&orphan.started_at),
            status: BatchJobStatus::Failed,
            error: Some(INTERRUPTED_ERROR_MESSAGE.to_string()),
            retry_count: 0,
        };
        super::io::record_terminal(conn, &entry)?;
        clear_inflight(conn, &orphan.job_id)?;
    }
    Ok(orphans.len())
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
            output_suffix: None,
        }
    }

    fn job(job_id: &str, status: BatchJobStatus) -> InFlightJob {
        InFlightJob {
            job_id: job_id.to_string(),
            batch_id: "batch1".to_string(),
            job_name: format!("{job_id}.mp4"),
            input_path: format!("/media/{job_id}.mp4"),
            config: config(),
            status,
            stage: "Editing".to_string(),
            started_at: "2026-09-06T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn upsert_then_list_round_trips_an_inflight_job() {
        let conn = open_in_memory().unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Editing)).unwrap();

        let all = list_inflight(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].job_id, "job1");
        assert_eq!(all[0].status, BatchJobStatus::Editing);
        assert_eq!(all[0].config, config());
    }

    #[test]
    fn upserting_the_same_job_id_again_overwrites_in_place_not_duplicates() {
        let conn = open_in_memory().unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Analyzing)).unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Rendering)).unwrap();

        let all = list_inflight(&conn).unwrap();
        assert_eq!(all.len(), 1, "same job_id must overwrite, not duplicate");
        assert_eq!(all[0].status, BatchJobStatus::Rendering);
    }

    #[test]
    fn clear_inflight_removes_the_row() {
        let conn = open_in_memory().unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Rendering)).unwrap();
        clear_inflight(&conn, "job1").unwrap();
        assert!(list_inflight(&conn).unwrap().is_empty());
    }

    #[test]
    fn clearing_an_unknown_job_id_is_a_harmless_no_op() {
        let conn = open_in_memory().unwrap();
        clear_inflight(&conn, "does-not-exist").unwrap();
        assert!(list_inflight(&conn).unwrap().is_empty());
    }

    #[test]
    fn list_inflight_orders_oldest_started_first() {
        let conn = open_in_memory().unwrap();
        let mut j1 = job("job1", BatchJobStatus::Editing);
        j1.started_at = "2026-09-05T00:00:00Z".to_string();
        let mut j2 = job("job2", BatchJobStatus::Editing);
        j2.started_at = "2026-09-01T00:00:00Z".to_string();
        upsert_inflight(&conn, &j1).unwrap();
        upsert_inflight(&conn, &j2).unwrap();

        let all = list_inflight(&conn).unwrap();
        assert_eq!(all[0].job_id, "job2", "the older job comes first");
        assert_eq!(all[1].job_id, "job1");
    }

    // -- recover_orphaned_jobs -------------------------------------------

    #[test]
    fn recover_orphaned_jobs_on_an_empty_table_recovers_nothing() {
        let conn = open_in_memory().unwrap();
        assert_eq!(recover_orphaned_jobs(&conn).unwrap(), 0);
    }

    #[test]
    fn recover_orphaned_jobs_converts_a_surviving_row_into_a_failed_history_entry() {
        let conn = open_in_memory().unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Rendering)).unwrap();

        let recovered = recover_orphaned_jobs(&conn).unwrap();
        assert_eq!(recovered, 1);

        // The row is gone from in_progress_jobs...
        assert!(list_inflight(&conn).unwrap().is_empty());

        // ...and a real Failed history entry now exists in its place, with
        // the honest interruption message and the original job's own
        // execution_plan (so it's genuinely retryable from scratch).
        let entry = super::super::io::get_history_entry(&conn, "job1")
            .unwrap()
            .expect("a history entry should now exist for the orphaned job");
        assert_eq!(entry.status, BatchJobStatus::Failed);
        assert_eq!(entry.error.as_deref(), Some(INTERRUPTED_ERROR_MESSAGE));
        assert_eq!(entry.execution_plan, config());
        assert_eq!(entry.input_path, "/media/job1.mp4");
        assert!(entry.output_path.is_none());
        assert!(entry.ended_at.is_some());
    }

    #[test]
    fn recover_orphaned_jobs_recovers_every_surviving_row_and_clears_each_one() {
        let conn = open_in_memory().unwrap();
        upsert_inflight(&conn, &job("job1", BatchJobStatus::Queued)).unwrap();
        upsert_inflight(&conn, &job("job2", BatchJobStatus::Transcribing)).unwrap();
        upsert_inflight(&conn, &job("job3", BatchJobStatus::Paused)).unwrap();

        let recovered = recover_orphaned_jobs(&conn).unwrap();
        assert_eq!(recovered, 3);
        assert!(list_inflight(&conn).unwrap().is_empty());

        for id in ["job1", "job2", "job3"] {
            let entry = super::super::io::get_history_entry(&conn, id)
                .unwrap()
                .expect("every orphaned job should get a history entry");
            assert_eq!(entry.status, BatchJobStatus::Failed);
        }
    }

    #[test]
    fn recover_orphaned_jobs_does_not_disturb_an_already_terminal_history_entry() {
        let conn = open_in_memory().unwrap();
        // A genuinely completed job's own real history row — untouched by
        // this table entirely (it was cleared from in_progress_jobs the
        // moment it completed, long before this recovery scan ever runs).
        let completed = HistoryEntry {
            id: "job-done".to_string(),
            batch_id: "batch1".to_string(),
            job_name: "done.mp4".to_string(),
            input_path: "/media/done.mp4".to_string(),
            output_path: Some("/media/batch_output/done_edited.mp4".to_string()),
            template_id: None,
            template_version: None,
            ai_prompt: None,
            ai_result: None,
            execution_plan: config(),
            capcut_draft_path: None,
            started_at: "2026-09-01T00:00:00Z".to_string(),
            ended_at: Some("2026-09-01T00:01:00Z".to_string()),
            duration_us: Some(60_000_000),
            status: BatchJobStatus::Completed,
            error: None,
            retry_count: 0,
        };
        super::super::io::record_terminal(&conn, &completed).unwrap();

        // A real orphan sitting alongside it.
        upsert_inflight(&conn, &job("job-orphan", BatchJobStatus::Editing)).unwrap();

        let recovered = recover_orphaned_jobs(&conn).unwrap();
        assert_eq!(recovered, 1, "only the real orphan should be recovered");

        let still_completed = super::super::io::get_history_entry(&conn, "job-done")
            .unwrap()
            .unwrap();
        assert_eq!(still_completed.status, BatchJobStatus::Completed);
        assert!(still_completed.error.is_none());
    }

    #[test]
    fn recovering_a_job_that_already_has_a_prior_failed_history_row_bumps_retry_count() {
        // A job that failed once for a real reason, was retried, and was
        // mid-flight again (Queued after `retry_batch_job`'s own reset) when
        // the app was killed a second time — its history row already exists
        // from the first failure; recovering it now must upsert in place
        // (same job_id), not duplicate.
        let conn = open_in_memory().unwrap();
        let first_failure = HistoryEntry {
            id: "job1".to_string(),
            batch_id: "batch1".to_string(),
            job_name: "job1.mp4".to_string(),
            input_path: "/media/job1.mp4".to_string(),
            output_path: None,
            template_id: Some("tmpl_tiktok".to_string()),
            template_version: Some(1),
            ai_prompt: None,
            ai_result: None,
            execution_plan: config(),
            capcut_draft_path: None,
            started_at: "2026-09-01T00:00:00Z".to_string(),
            ended_at: Some("2026-09-01T00:01:00Z".to_string()),
            duration_us: Some(60_000_000),
            status: BatchJobStatus::Failed,
            error: Some("ffmpeg exited with a non-zero status".to_string()),
            retry_count: 0,
        };
        super::super::io::record_terminal(&conn, &first_failure).unwrap();

        upsert_inflight(&conn, &job("job1", BatchJobStatus::Queued)).unwrap();
        recover_orphaned_jobs(&conn).unwrap();

        let entry = super::super::io::get_history_entry(&conn, "job1")
            .unwrap()
            .unwrap();
        assert_eq!(entry.status, BatchJobStatus::Failed);
        assert_eq!(entry.error.as_deref(), Some(INTERRUPTED_ERROR_MESSAGE));
        assert_eq!(
            entry.retry_count, 1,
            "recovering re-uses the same history row id, so it counts as a retry"
        );
    }
}
