//! `BatchJob`/`BatchPipelineConfig` — the data schema batch processing
//! exposes to the frontend (master prompt §42/§43). See `batch` module doc
//! comment for the full design writeup.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::captions::generate::CaptionGenerationSettings;
use crate::vad::CutParams;

/// Closed state enum. Matches the master prompt's own Jobs UI state list
/// (`Queued`/`Analyzing`/`Transcribing`/`Editing`/`Rendering`/`Completed`/
/// `Failed`/`Cancelled`) with one addition: **`Paused`**.
///
/// The master prompt's own state list has no separate "Paused" row, but its
/// "Allow" list separately names `pause` as an action distinct from any one
/// processing state — pausing is legal while `Analyzing`/`Transcribing`/
/// `Editing`/`Rendering`, or even while still `Queued`. Two designs were
/// available: (a) a `paused: bool` flag layered on top of whichever
/// processing state was active, or (b) `Paused` as its own closed-enum
/// variant. This module picks **(b)**: the required Jobs UI "Status" column
/// (master prompt §42) needs exactly one value per row, and a flag-on-top-of-
/// state design would force the frontend to combine two independent fields
/// into that one cell. The stage the job will resume from is not lost by
/// this choice — `BatchJob::stage` keeps showing the last real stage label
/// (e.g. `"Editing"`) while `status` is `Paused`, and the manager's own pause
/// checkpoint (module doc comment, `batch::pipeline`) always resumes at the
/// next real stage boundary regardless of which enum shape recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BatchJobStatus {
    Queued,
    Analyzing,
    Transcribing,
    Editing,
    Rendering,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl BatchJobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            BatchJobStatus::Completed | BatchJobStatus::Failed | BatchJobStatus::Cancelled
        )
    }

    /// Whether this status represents a real, currently-running processing
    /// stage (as opposed to `Queued`/`Paused`/a terminal state) — used to
    /// decide whether an ETA estimate is even meaningful.
    pub fn is_actively_processing(self) -> bool {
        matches!(
            self,
            BatchJobStatus::Analyzing
                | BatchJobStatus::Transcribing
                | BatchJobStatus::Editing
                | BatchJobStatus::Rendering
        )
    }
}

/// One item in a batch (master prompt §42's Jobs UI row: Name/Status/
/// Progress/Stage/Elapsed/ETA/Output). `elapsed_us`/`eta_us` are computed
/// fresh every time a snapshot is taken (`batch::manager::JobState::snapshot`)
/// from `started_at` and the current progress — never separately mutated
/// fields that could drift out of sync.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BatchJob {
    pub id: String,
    /// The source filename (master prompt §42's own "Name" column example).
    pub name: String,
    pub status: BatchJobStatus,
    /// `0.0..=1.0`.
    pub progress: f32,
    /// Human-readable current-step label (e.g. `"Removing silence"`,
    /// `"Rendering"`) — finer-grained than `status` alone, matching the
    /// master prompt's separate "Stage" column.
    pub stage: String,
    /// RFC3339 timestamp of when this job actually started running (left
    /// `Queued` and entered `Analyzing`) — not when the batch itself was
    /// created. A job that is still `Queued` reports its creation time here
    /// (overwritten the moment it starts) with `elapsed_us: 0`.
    pub started_at: String,
    /// Microseconds from `started_at` to now (still-running) or to the
    /// moment it reached a terminal state (frozen thereafter).
    pub elapsed_us: i64,
    /// A real, extrapolated-from-progress-so-far estimate, or `None` when
    /// there isn't yet enough signal to extrapolate honestly (progress is
    /// still `0.0`, the job hasn't started, or it's already finished) — per
    /// this feature's own requirement, never a fabricated precise number.
    pub eta_us: Option<i64>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

/// Which stages a batch runs, and their real parameters — every field here
/// is either an existing, real settings type from elsewhere in this
/// codebase, or references one by id (module doc comment). Every stage is
/// independently toggleable via `Option`/`None` = skip, except rendering/
/// export, which always runs (a batch item with no output isn't really a
/// "batch job" in the master prompt's own Jobs-UI-with-an-Output-column
/// sense).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct BatchPipelineConfig {
    /// `None` skips the whole silence-removal stage. When both this and
    /// `template_id` are given, this explicit value always wins over the
    /// template's own `silence_settings` (see `template_id`'s doc comment).
    pub remove_silence: Option<CutParams>,
    /// `None` skips caption generation entirely, and with it the whole
    /// Transcribing stage (module doc comment: transcription only runs when
    /// a downstream stage — captioning, here — actually needs a transcript).
    pub captions: Option<CaptionGenerationSettings>,
    /// Required (and validated up front, before any real work starts)
    /// whenever `captions` is `Some` — captioning needs a transcript, and
    /// transcription needs a specific installed Whisper model
    /// (`commands::transcription::transcribe_media`'s own required
    /// parameter). Ignored when `captions` is `None`.
    pub transcription_model_id: Option<String>,
    /// Optional forced transcription language (Whisper's own "auto-detect
    /// when `None`" behavior otherwise). Ignored when `captions` is `None`.
    pub transcription_language: Option<String>,
    /// References `templates::all_templates()`'s stable id, or a custom
    /// template's `custom_<uuid>` id (`templates::io`). When set, the
    /// template's `canvas` is applied to every batch item's project, its
    /// `caption_style` is applied to generated captions (only meaningful
    /// when `captions` is also `Some`), and its `silence_settings` becomes
    /// the *default* `remove_silence` value when the caller left that field
    /// `None` (an explicit `remove_silence` always overrides it). `None` =
    /// no template applied at all.
    ///
    /// Honest scope note: this pipeline does **not** apply
    /// `Template::zoom_intensity`/`transition_settings`/`sports_overlay` —
    /// doing so correctly for zoom would require re-deriving
    /// `media::scene`/`zoom`'s trigger detection against the *post-silence-
    /// cut* multi-fragment timeline (the same source-time -> timeline-time
    /// remapping this module already builds for captions,
    /// `batch::pipeline::remap_transcript_across_fragments`, but for
    /// keyframes instead of captions), which is real additional complexity
    /// out of scope for this pass. `transition_settings`/`sports_overlay`
    /// remain structural-only even in `templates` itself (that module's own
    /// doc comment) — there is nothing working to apply yet.
    pub template_id: Option<String>,
    /// References `render::presets::all_presets()`. Required unless
    /// `template_id` is given, in which case the template's own
    /// `export_preset_id` is used as the fallback (same precedence rule as
    /// `remove_silence` above: an explicit value here always wins).
    pub export_preset_id: Option<String>,
    /// Overrides the `<stem>_<suffix>.<ext>` suffix `batch::pipeline`'s own
    /// `default_output_path` uses when naming this job's rendered output.
    /// `None` keeps this pipeline's original, single-template default —
    /// `"edited"` (`video01_edited.mp4`) — unchanged for every existing
    /// caller. Multi-template batches (`batch::manager::start_multi_template_batch`,
    /// upgrade-plan §11) set this to the target template's own
    /// filesystem-safe slug (`batch::pipeline::slugify_template_name`,
    /// e.g. `"TikTok"` -> `"tiktok"`) so N videos x M templates land as N x M
    /// distinctly-named files (`video01_tiktok.mp4`, `video01_youtube.mp4`,
    /// ...) instead of colliding on one shared `video01_edited.mp4` per
    /// video. Not template-derived automatically even when `template_id` is
    /// set — an explicit, caller-chosen value, exactly like every other
    /// field in this struct.
    pub output_suffix: Option<String>,
}
