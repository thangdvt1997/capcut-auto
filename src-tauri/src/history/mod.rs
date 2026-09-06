//! Video Processing History (upgrade-plan §21 / `UPGRADE_PLAN.md` Phase U3):
//! a real, durable-across-restarts record of every batch job that reached a
//! terminal state (`Completed`/`Failed`/`Cancelled`), queryable and
//! re-runnable — the thing `batch::manager::BatchJobManager`'s own in-memory
//! `HashMap<String, JobHandle>` cannot give you once the app restarts (that
//! state really is lost on exit — verified directly: nothing in `manager.rs`
//! persists it anywhere).
//!
//! ## Storage-location decision
//!
//! This module deliberately does **not** open a third database file or
//! connection. `db::MediaLibrary` already manages exactly the kind of thing
//! this table needs — a bundled-`rusqlite` connection, guarded by one
//! `Mutex`, opened once as Tauri-managed state (`commands::media::init_media_library`)
//! — so the `history` table lives in the *same* `media_library.sqlite3` file,
//! behind the *same* `Mutex<Connection>`, rather than standing up a second
//! connection to coordinate (two locks over one file buys nothing here: both
//! tables are small, local, single-writer-per-command data, exactly what
//! `MediaLibrary`'s own doc comment already argues for holding one mutex
//! over). `crate::history::io::init_schema` is called once at startup
//! (`lib.rs`'s `run()` setup, right after `init_media_library`) against that
//! same connection. The Tauri state type is still named `MediaLibrary` (it
//! predates this table) — not renamed here, since that would touch every
//! existing `commands::media::*` call site for a purely cosmetic reason;
//! `commands::history`'s own doc comment repeats this reuse decision so it
//! isn't a silent surprise to a future reader who only opens that file.
//!
//! ## Which §21 fields are real vs. structurally-present-only
//!
//! - **Real, populated from this pass's own wiring**: `input_path`,
//!   `output_path`, `template_id`, `started_at`/`ended_at`/`duration_us`,
//!   `status`, `error`, `retry_count`.
//! - **`template_version`**: real, but with one honestly-documented narrow
//!   race — see [`HistoryEntry::template_version`]'s own doc comment.
//! - **`execution_plan`**: real — the exact `BatchPipelineConfig` (Phase 11)
//!   that shaped that job's editing *and* rendering — see its own doc
//!   comment for why this one field stands in for §21's *both*
//!   "Editing plan" and "Execution plan" rows.
//! - **`ai_prompt`/`ai_result`**: structurally present (closed `Option<String>`
//!   fields, specta-typed, real DB columns), always `None` from every job
//!   this pipeline runs today — `batch::pipeline::run_pipeline` does not
//!   invoke any AI step itself (checked before writing this module: no
//!   `ai::smart_edit`/`ai::edit_plan`/`ai::auto_template` call anywhere in
//!   that file). Populating them for real is scoped to whichever future pass
//!   actually wires an AI step into the batch pipeline, not this one.
//! - **`capcut_draft_path`**: structurally present, always `None`. Per
//!   `UPGRADE_PLAN.md`'s own audit note, this app's CapCut integration is
//!   direct draft-file export (`capcut::export`), not a live "CapCut worker"
//!   process — §21's own "CapCut worker" field has no honest equivalent here
//!   (there is no worker id/process to record) and is not fabricated. A
//!   batch job's pipeline does not itself call `capcut::export` as of this
//!   pass (nothing in `batch::pipeline::run_pipeline` does), so this field
//!   would always be `None` today regardless — kept as the one honest,
//!   forward-looking rename of that spec field, not the spec's own concept.
//!
//! ## Retry / re-run semantics
//!
//! A `HistoryEntry`'s `id` is exactly the batch job's own `job_id`
//! (`batch::manager::JobState::id`) — not a separately-minted uuid. This is
//! what makes "retry increments `retry_count` on the *same* logical history
//! entry" fall out for free: `batch::manager::prepare_retry` resets a
//! `Failed` job's state back to `Queued` **in place** (same `job_id`,
//! `batch` module doc comment's own documented "no per-stage checkpointing,
//! re-run from the start" retry semantics) and re-runs it, so the next time
//! that job reaches a terminal state, `history::io::record_terminal` upserts
//! the *same* row (`ON CONFLICT(id)`) and bumps `retry_count` rather than
//! inserting a second, disconnected row. A **re-run** (§21's own separate
//! "Re-run" action) is different on purpose: it starts a *brand-new* batch
//! job via `BatchJobManager::create_batch` (a fresh `job_id`), which will
//! earn its own, separate `HistoryEntry` once *it* finishes — re-running an
//! old job is not the same thing as retrying a still-tracked one, and this
//! module never conflates the two.

pub mod error;
pub mod io;

pub use error::HistoryError;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::batch::{BatchJobStatus, BatchPipelineConfig};

/// One durable row: everything §21 asks a Video Processing History to keep
/// about one batch job. See this module's own doc comment for the full
/// storage-location/field-honesty writeup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    /// Exactly the originating `BatchJob::id` — see module doc comment's
    /// "Retry / re-run semantics" section for why this, not a separate uuid.
    pub id: String,
    /// The batch this job was created as part of (`batch::manager`'s own
    /// `batch_order`/`job_batch` bookkeeping) — not one of §21's own listed
    /// fields, but free to record (the manager already tracks it) and useful
    /// for a frontend that wants to group history rows by the batch run they
    /// came from.
    pub batch_id: String,
    pub job_name: String,
    pub input_path: String,
    pub output_path: Option<String>,
    pub template_id: Option<String>,
    /// Resolved fresh (built-in-then-custom two-tier lookup,
    /// `batch::pipeline::resolve_template_version`) at the moment this job's
    /// history row is written — i.e. right after it reaches a terminal
    /// state, using the same `templates_dir` the pipeline itself just ran
    /// against. Honest narrow gap: `batch::pipeline::run_pipeline`'s own
    /// template resolution happens once, right at the *start* of the job
    /// (its "upfront validation" section), and its resolved `Template` value
    /// is not itself threaded back out through `run_pipeline`'s return type
    /// (which would be a wider, more invasive change than this pass's scope
    /// — several existing tests destructure `run_pipeline`'s `Ok` value as a
    /// bare `PathBuf`). So if a custom template is edited *while* one of its
    /// own jobs is still mid-render, this field reflects the version current
    /// at job-*completion* time, not necessarily the exact version the
    /// render itself used moments earlier. A narrow, documented race, not a
    /// silently wrong answer — built-ins never have this problem (always
    /// version 1, per `templates` module's own versioning design).
    pub template_version: Option<u32>,
    /// Always `None` from this pipeline today — see module doc comment.
    pub ai_prompt: Option<String>,
    /// Always `None` from this pipeline today — see module doc comment.
    pub ai_result: Option<String>,
    /// The real `BatchPipelineConfig` this job ran with — §21's "Editing
    /// plan" and "Execution plan" rows collapsed into this one real,
    /// already-serializable type, since this pipeline has no separate
    /// AI-authored `EditPlan`-shaped object per job (`ai::edit_plan::EditPlan`
    /// exists elsewhere in this codebase, but `batch::pipeline::run_pipeline`
    /// never constructs or consumes one) — `BatchPipelineConfig` genuinely
    /// *is* both concepts here: it's what shaped both the editing
    /// (silence-removal/caption settings) and the execution/render
    /// (export preset, template, output naming) for this exact job. Stored
    /// as a JSON TEXT column (`history::io`), not a second bespoke plan
    /// type.
    pub execution_plan: BatchPipelineConfig,
    /// Always `None` — see module doc comment's "CapCut worker" writeup.
    pub capcut_draft_path: Option<String>,
    /// RFC3339.
    pub started_at: String,
    /// RFC3339, `None` only if a row were ever read before its job finished
    /// (never actually happens — `record_terminal` is only ever called once
    /// a job is already terminal).
    pub ended_at: Option<String>,
    pub duration_us: Option<i64>,
    pub status: BatchJobStatus,
    pub error: Option<String>,
    /// `0` the first time this job reaches a terminal state; incremented by
    /// `history::io::record_terminal`'s own upsert every subsequent time
    /// (i.e. once per retry that reaches a terminal state again) — never
    /// set directly by a caller (see that function's doc comment).
    pub retry_count: u32,
}

/// Real logic behind `commands::history::rerun_from_history` (§21's own
/// "Re-run" action): the exact `BatchPipelineConfig` a re-run job should
/// start with — `entry`'s own `execution_plan`, unchanged. Split out as its
/// own pure function (rather than inlined in the command) so it's directly
/// unit-testable, and so `build_rerun_with_template_config` below can be
/// tested against exactly the same "what does a re-run actually reuse"
/// question. The command layer takes this config and calls
/// `BatchJobManager::create_batch` with it (Phase 11's own real batch
/// creation, reused unchanged — never a parallel "resume the old job"
/// mechanism) plus `batch::manager::spawn_batch_worker`, both of which need
/// a running `AppHandle`/Tauri runtime this pure function deliberately does
/// not (this crate's own tests have no way to construct a real `AppHandle`
/// outside a running app, matching every other AppHandle-dependent
/// batch-starting function in this codebase, e.g. `batch::manager::start_batch`,
/// which is likewise untested directly — only its own AppHandle-free
/// `create_batch` core is). The old `entry` itself is never touched or
/// referenced again by a re-run; the new job earns its own fresh history row
/// once *it* finishes (module doc comment's "Retry / re-run semantics").
pub fn build_rerun_config(entry: &HistoryEntry) -> BatchPipelineConfig {
    entry.execution_plan.clone()
}

/// Same as [`build_rerun_config`], but with `template_id` swapped for
/// `new_template_id` (§21's "Run with another template" action) —
/// everything else about the job's original `execution_plan` (silence
/// removal, captions, export preset, output suffix) is kept exactly as it
/// was. Deliberately does **not** validate `new_template_id` up front
/// against the real template catalog — the same "resolved lazily, inside
/// the job itself, failing that one job with a clear
/// `BatchError::UnknownTemplate` if it's wrong" posture
/// `batch::manager::create_batch`/`start_batch` already have for a
/// single-template batch (only `start_multi_template_batch`'s N x M fan-out
/// validates every id up front, since one bad id there would otherwise
/// pre-doom a whole batch of jobs deep into a possibly-long run — a single
/// re-run job has no such blast radius to protect against).
pub fn build_rerun_with_template_config(
    entry: &HistoryEntry,
    new_template_id: String,
) -> BatchPipelineConfig {
    BatchPipelineConfig {
        template_id: Some(new_template_id),
        ..entry.execution_plan.clone()
    }
}

/// A real, named specta-typed struct — not a bare tuple — for the same
/// reason every other multi-field command return in this codebase
/// (`BatchProgressEvent`) is a struct: a tuple loses its field names
/// crossing the Tauri IPC boundary into TypeScript, a struct doesn't.
#[derive(Debug, Clone, Serialize, Type)]
pub struct RerunResult {
    pub batch_id: String,
    pub job_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vad::CutParams;

    fn minimal_config() -> BatchPipelineConfig {
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

    fn sample_entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            batch_id: "batch1".to_string(),
            job_name: "clip.mp4".to_string(),
            input_path: "/media/clip.mp4".to_string(),
            output_path: Some("/media/batch_output/clip_edited.mp4".to_string()),
            template_id: Some("tmpl_tiktok".to_string()),
            template_version: Some(1),
            ai_prompt: None,
            ai_result: None,
            execution_plan: minimal_config(),
            capcut_draft_path: None,
            started_at: "2026-09-06T00:00:00Z".to_string(),
            ended_at: Some("2026-09-06T00:01:00Z".to_string()),
            duration_us: Some(60_000_000),
            status: BatchJobStatus::Completed,
            error: None,
            retry_count: 0,
        }
    }

    #[test]
    fn build_rerun_config_returns_the_entrys_own_execution_plan_unchanged() {
        let entry = sample_entry("job1");
        let config = build_rerun_config(&entry);
        assert_eq!(config, entry.execution_plan);
    }

    #[test]
    fn build_rerun_with_template_config_swaps_only_the_template_id() {
        let entry = sample_entry("job1");
        let config = build_rerun_with_template_config(&entry, "tmpl_youtube_shorts".to_string());

        assert_eq!(config.template_id.as_deref(), Some("tmpl_youtube_shorts"));
        // Everything else about the original plan is preserved untouched.
        assert_eq!(config.remove_silence, entry.execution_plan.remove_silence);
        assert_eq!(config.captions, entry.execution_plan.captions);
        assert_eq!(
            config.transcription_model_id,
            entry.execution_plan.transcription_model_id
        );
        assert_eq!(
            config.export_preset_id,
            entry.execution_plan.export_preset_id
        );
        assert_eq!(config.output_suffix, entry.execution_plan.output_suffix);
    }

    #[test]
    fn build_rerun_with_template_config_does_not_mutate_the_original_entry() {
        let entry = sample_entry("job1");
        let original_template_id = entry.execution_plan.template_id.clone();
        let _ = build_rerun_with_template_config(&entry, "tmpl_news".to_string());
        assert_eq!(entry.execution_plan.template_id, original_template_id);
    }
}
