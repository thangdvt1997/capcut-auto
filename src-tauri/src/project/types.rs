//! `ProjectV1` struct tree — a direct Rust encoding of the schema documented
//! in `docs/project-format.md`. Field names match the JSON schema exactly
//! (both already use snake_case), so `#[serde(rename...)]` is only needed
//! where a JSON value isn't a valid Rust identifier (e.g. `"16:9"`).
//!
//! Timebase: every duration/position/offset is `i64` microseconds
//! (`_us` suffix), per master prompt §67 and `docs/architecture-audit.md`
//! §1/§5. Do not introduce a float-seconds or millisecond field here —
//! conversion to FFmpeg seconds / FCPXML rational frames happens only at
//! those adapters' boundaries (`docs/architecture.md` "Timebase conversion
//! boundaries"), never in this core schema.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A rational number, used for frame rates (`num/den`, e.g. 30000/1001 for
/// 29.97 fps) to avoid the float-drift problems documented in autocut's
/// `timecode.rs` (audit §2/§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Rational {
    pub num: u32,
    pub den: u32,
}

impl Rational {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }
}

/// Standard 30 fps (30000/1001 is the NTSC-accurate default; a plain 30/1
/// keeps the Phase 2 default project simple. Real fps detection happens at
/// media-import time, Phase 3.)
impl Default for Rational {
    fn default() -> Self {
        Self::new(30, 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CanvasRatioPreset {
    #[serde(rename = "16:9")]
    Ratio16x9,
    #[serde(rename = "9:16")]
    Ratio9x16,
    #[serde(rename = "1:1")]
    Ratio1x1,
    #[serde(rename = "4:5")]
    Ratio4x5,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CanvasV1 {
    pub width: u32,
    pub height: u32,
    pub fps: Rational,
    pub ratio_preset: CanvasRatioPreset,
}

impl Default for CanvasV1 {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: Rational::new(30000, 1001),
            ratio_preset: CanvasRatioPreset::Ratio16x9,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    /// RFC3339 timestamp.
    pub created_at: String,
    /// RFC3339 timestamp.
    pub modified_at: String,
    /// semver of the app version that wrote this file.
    pub app_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MediaItem {
    pub id: String,
    pub kind: MediaKind,
    /// Absolute or project-relative path.
    pub source_path: String,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub fps: Rational,
    pub codec: String,
    pub bitrate: i64,
    pub audio_channels: u16,
    pub sample_rate: u32,
    pub rotation_deg: i32,
    /// RFC3339 timestamp, or `None` if the source has no embedded creation
    /// time.
    pub created_at: Option<String>,
    pub proxy_path: Option<String>,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
    Caption,
    Image,
    Overlay,
    Effect,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    /// Stacking order; higher draws on top (pyJianYingDraft convention,
    /// audit §1).
    pub render_index: i32,
    pub locked: bool,
    pub hidden: bool,
    pub muted: bool,
    pub solo: bool,
    /// Ordered; the clips themselves live in `ProjectV1::clips`, keyed by
    /// id — never store clip data inline here (stable-ID references only,
    /// master prompt §5).
    pub clip_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ClipSettings {
    pub opacity: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub rotation_deg: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    /// Half-canvas-width/height units, matching pyJianYingDraft's
    /// `transform_x`/`transform_y` convention (audit §1) — NOT pixels. A
    /// frequent bug source upstream; documented here so it isn't
    /// rediscovered the hard way in the render/capcut adapters.
    pub transform_x: f64,
    pub transform_y: f64,
}

impl Default for ClipSettings {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            flip_h: false,
            flip_v: false,
            rotation_deg: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            transform_x: 0.0,
            transform_y: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Clip {
    pub id: String,
    pub track_id: String,
    /// `None` for e.g. a pure-effect or generated-caption clip.
    pub media_id: Option<String>,
    /// Trim into the source media.
    pub source_in_us: i64,
    pub source_out_us: i64,
    /// Placement on the track's timeline.
    pub position_us: i64,
    pub speed: f64,
    pub enabled: bool,
    /// `SyncGroup` membership, see `SyncGroup` below.
    pub group_id: Option<String>,
    pub clip_settings: ClipSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Word {
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Caption {
    pub id: String,
    pub track_id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
    pub words: Vec<Word>,
    pub style_id: Option<String>,
}

/// Sentence/segment-level transcript entry, extended (Phase 7, master
/// prompt §14 "Prefer word-level timestamps") with a `words` breakdown —
/// additive, pre-1.0 internal schema evolution, mirroring `Caption`'s
/// existing `words: Vec<Word>` field above rather than inventing a second
/// shape for the same idea. Empty (`vec![]`) for any entry produced before
/// this field existed, or by a transcription provider that only reports
/// segment-level timing.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TranscriptEntry {
    pub id: String,
    pub media_id: String,
    pub text: String,
    pub start_us: i64,
    pub end_us: i64,
    pub confidence: f32,
    pub words: Vec<Word>,
    pub is_filler: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Effect {
    pub id: String,
    pub clip_id: String,
    pub kind: String,
    /// Freeform, effect-kind-specific parameters. Kept as opaque JSON here
    /// deliberately — the effect catalog/parameter schemas don't exist yet
    /// (Phase 6+), so a closed Rust struct would just be guessing.
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AnimationKind {
    In,
    Out,
    Loop,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Animation {
    pub id: String,
    pub clip_id: String,
    pub kind: AnimationKind,
    pub name: String,
    pub duration_us: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Keyframe {
    pub id: String,
    pub clip_id: String,
    /// One of `position_x | position_y | rotation | scale | alpha | volume`
    /// and more as the property catalog grows (`docs/project-format.md`
    /// leaves this open-ended) — kept as `String` rather than a closed enum
    /// so adding a keyframeable property doesn't require a schema-breaking
    /// migration.
    pub property: String,
    pub time_offset_us: i64,
    pub value: f64,
    /// pyJianYingDraft hardcodes linear-only interpolation today (audit
    /// §1); `curve` is a `String` (not a unit enum) so bezier/ease support
    /// can be added later without another schema version bump.
    pub curve: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CutKind {
    Remove,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CutReason {
    Silence,
    FillerWord,
    AiSuggested,
}

/// Edit-plan / silence-removal provenance — NOT a duplicate timeline.
/// Records *why* clips were split/removed by an automated pass (VAD, AI
/// EditPlan), for undo/audit/re-analysis (`docs/project-format.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Cut {
    pub id: String,
    pub kind: CutKind,
    pub source_media_id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub reason: CutReason,
    pub applied: bool,
}

/// Generalizes autocut's fixed-camera-rig "shared cutlist + per-track
/// offset" concept (audit §4) into a first-class, optional relationship. A
/// clip's `Clip::group_id` points at a `SyncGroup`; the timeline engine
/// (Phase 4) propagates split/trim/delete across all members unless the
/// user explicitly ungroups.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SyncGroup {
    pub id: String,
    /// Clips that must move/trim/cut together.
    pub clip_ids: Vec<String>,
    /// Relative alignment, keyed by clip id.
    pub offsets_us: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
pub struct AiState {
    /// Opaque id — actual credentials live in the Windows Credential
    /// Manager, never in `project.json` (master prompt §17, Phase 10).
    pub provider_settings_ref: Option<String>,
    /// Most recent `EditPlan` JSON (`docs/ai-engine.md`, Phase 10). Opaque
    /// here for the same reason as `Effect::params`: the schema doesn't
    /// exist yet.
    pub last_edit_plan: Option<serde_json::Value>,
    pub highlights: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
pub struct ExportState {
    pub last_render_preset: Option<String>,
    pub last_capcut_draft_path: Option<String>,
}

/// The unified project file (`project.json`). See `docs/project-format.md`
/// for the full design rationale — stable UUID-based ids everywhere,
/// non-destructive (never stores rendered pixels/audio), versioned for
/// forward migration.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProjectV1 {
    /// Always `1` for this schema version. `ProjectV1::migrate_to_latest`
    /// dispatches on this field.
    pub version: u32,
    pub project: ProjectMeta,
    pub canvas: CanvasV1,
    pub media: Vec<MediaItem>,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub captions: Vec<Caption>,
    pub transcript: Vec<TranscriptEntry>,
    pub effects: Vec<Effect>,
    pub animations: Vec<Animation>,
    pub keyframes: Vec<Keyframe>,
    pub cuts: Vec<Cut>,
    pub ai: AiState,
    pub export: ExportState,
    pub sync_groups: Vec<SyncGroup>,
}

impl ProjectV1 {
    pub const SCHEMA_VERSION: u32 = 1;

    /// A brand-new, empty project with sensible defaults. Does not touch
    /// the filesystem — see `project::io` for atomic save/load.
    pub fn new(name: impl Into<String>) -> Self {
        let now = crate::project::io::now_rfc3339();
        Self {
            version: Self::SCHEMA_VERSION,
            project: ProjectMeta {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.into(),
                created_at: now.clone(),
                modified_at: now,
                app_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            canvas: CanvasV1::default(),
            media: Vec::new(),
            tracks: Vec::new(),
            clips: Vec::new(),
            captions: Vec::new(),
            transcript: Vec::new(),
            effects: Vec::new(),
            animations: Vec::new(),
            keyframes: Vec::new(),
            cuts: Vec::new(),
            ai: AiState::default(),
            export: ExportState::default(),
            sync_groups: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_has_schema_version_1() {
        let p = ProjectV1::new("Test");
        assert_eq!(p.version, 1);
        assert_eq!(p.project.name, "Test");
        assert!(!p.project.id.is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let mut p = ProjectV1::new("Round Trip");
        p.tracks.push(Track {
            id: "t1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(Clip {
            id: "c1".into(),
            track_id: "t1".into(),
            media_id: None,
            source_in_us: 0,
            source_out_us: 5_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });

        let json = serde_json::to_string_pretty(&p).expect("serialize");
        let back: ProjectV1 = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.version, p.version);
        assert_eq!(back.tracks.len(), 1);
        assert_eq!(back.clips[0].source_out_us, 5_000_000);
    }

    #[test]
    fn canvas_ratio_preset_serializes_to_documented_json_values() {
        let json = serde_json::to_value(CanvasRatioPreset::Ratio16x9).unwrap();
        assert_eq!(json, serde_json::json!("16:9"));
    }
}
