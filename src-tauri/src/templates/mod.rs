//! Templates system (master prompt §36 "Templates" / §37 "Football/Sports
//! template"): a `Template` is a named, reusable bundle of settings this
//! codebase already has real, working types for — canvas (`project::CanvasV1`,
//! Phase 2), caption style (`project::CaptionStyle`, Phase 8 — see
//! `captions::styles::all_caption_templates` for the exact "catalog function
//! returning fixed-id, owned built-ins" pattern this module mirrors, one
//! level up: a `Template` really is "a bundle of bundles"), zoom intensity
//! (`zoom::ZoomIntensity`, Phase 11), silence-removal padding/merge
//! (`vad::cutlist::CutParams`, Phase 5), an export preset id (referencing
//! `render::presets::all_presets()`'s stable ids, Phase 6 — never a
//! duplicated copy of `RenderSettings`), and a small, honest AI-prompt-seeding
//! config (`AiPromptConfig`, below).
//!
//! ## What's real vs. structural-only here
//!
//! Every field above maps onto a mechanism that already runs for real
//! elsewhere in this codebase. Two exceptions, documented rather than faked
//! as working:
//!
//! - **`transition_settings`** (master prompt §36's "transition settings" /
//!   §37's "fast transitions"): `render::plan`/`render::graph` have no
//!   time-varying cross-clip blending concept as of this pass (checked
//!   before writing this module — no cross-fade/wipe/etc. filter chain
//!   exists anywhere in `render`) — no transition ever actually renders yet;
//!   every real render today is an implicit hard cut between adjacent
//!   clips. [`TransitionSettings`] is kept as a genuine structural field (a
//!   [`TransitionType`] enum plus a duration) recording the *intended*
//!   behavior for whichever future render-engine pass adds real transition
//!   compositing — not a working blend today.
//! - **`SportsOverlaySettings::score_overlay_suggested`** (§37's "optional
//!   score/title overlay"): no overlay-rendering engine exists yet either —
//!   `project::types::Effect::params` is still opaque JSON (per that type's
//!   own doc comment, "the effect catalog/parameter schemas don't exist
//!   yet") — this is a UI-seed boolean only, not a working overlay renderer.
//!
//! Everything else §37 lists (highlight markers, slow-motion sections,
//! replay markers, music track, logo overlay) maps onto mechanisms that are
//! *already fully real* and need no new `Template` field at all — see
//! [`SportsOverlaySettings`]'s doc comment for exactly which existing type
//! each one reuses.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ai::smart_edit::SmartEditCategory;
use crate::captions::styles;
use crate::project::{
    AudioRole, CanvasRatioPreset, CanvasV1, CaptionStyle, DuckingSettings, ProjectV1, Rational,
};
use crate::render::find_preset;
use crate::vad::CutParams;
use crate::zoom::ZoomIntensity;

pub mod error;
pub mod io;

pub use error::TemplateError;

/// Every template ever produced by [`all_templates`] starts at this version
/// (upgrade spec §20) — see [`Template::version`] doc comment.
fn default_template_version() -> u32 {
    1
}

/// A `Template`'s reference to one [`crate::assets::Asset`] by id (upgrade
/// spec §17's "template reference asset bằng ID thay vì hard-code path"
/// requirement) — used as-is for `Template::intro`/`outro`, which need no
/// per-use override; [`WatermarkReference`]/[`BackgroundMusicReference`]
/// below wrap the same `asset_id` with the one or two overrides §3's own
/// example (`position`/`volume`) calls for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AssetReference {
    pub asset_id: String,
}

/// Corner/center placement for a watermark or logo overlay (upgrade spec
/// §3's own `position: top-right` example). Deliberately a new, small enum
/// rather than reusing `project::CaptionAnchor` (vertical-only:
/// top/center/bottom, no left/right) or `project::CaptionAlignment`
/// (horizontal-only, meant for multi-line text alignment) — neither already
/// expresses a 2D corner, and no overlay-rendering engine exists yet to
/// consume a richer offset-based placement (see `assets::mod`'s own
/// `Watermark`-is-structural-until-an-overlay-engine-exists note).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WatermarkReference {
    pub asset_id: String,
    pub position: WatermarkPosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct BackgroundMusicReference {
    pub asset_id: String,
    /// Linear gain multiplier, same convention as
    /// `project::AudioClipSettings::volume` (upgrade spec §3's own example
    /// literally writes `volume: 0.2`, matching this linear-gain
    /// convention) — NOT a 0-100 percentage and NOT decibels.
    pub volume: f64,
}

/// Structural-only placeholder for a render-time cross-clip transition (see
/// module doc comment) — `Cut` is the only one every render actually
/// performs today (an implicit hard cut between adjacent clips); `CrossFade`
/// records *intent* for a future `render::graph` blending pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransitionType {
    #[default]
    Cut,
    CrossFade,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct TransitionSettings {
    pub transition_type: TransitionType,
    /// Only meaningful once `transition_type` is not `Cut` — how long the
    /// (not-yet-implemented, see module doc comment) blend would take.
    pub duration_us: i64,
}

impl TransitionSettings {
    pub const fn cut() -> Self {
        Self {
            transition_type: TransitionType::Cut,
            duration_us: 0,
        }
    }

    pub const fn cross_fade(duration_us: i64) -> Self {
        Self {
            transition_type: TransitionType::CrossFade,
            duration_us,
        }
    }
}

/// Small, honest AI-prompt-seeding defaults (module doc comment) — NOT a new
/// AI capability. `emphasized_categories` seeds which
/// `ai::smart_edit::SmartEditCategory`s a caller should pre-select/prioritize
/// in the Smart Edit UI for this genre of content; `system_prompt_prefix`
/// (optional) is meant to be prepended ahead of
/// `ai::nl_command::build_edit_plan_prompt`'s own generated system prompt by
/// the caller, unchanged — this module never constructs an `AiRequest` or
/// calls an `AIProvider` itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, Default)]
pub struct AiPromptConfig {
    pub emphasized_categories: Vec<SmartEditCategory>,
    pub system_prompt_prefix: Option<String>,
}

/// Football/Sports-template-only optional settings (master prompt §37),
/// `None` on every non-sports `Template`. See the module doc comment for
/// which of these are real vs. structural. §37 bullets *not* represented as
/// a field here, because they already map onto existing, unmodified
/// mechanisms with no new field needed:
/// - **Slow-motion sections**: `project::Clip::speed < 1.0` (real, Phase 9,
///   already consumed by `render::plan`'s speed filters) — a per-clip
///   property set on the timeline after a project is built from this
///   template, not a template-level setting.
/// - **Highlight markers / replay markers**: Phase 10's real
///   `highlights::types::Highlight` (persisted at `ProjectV1::ai::highlights`) —
///   running highlight detection (`commands::highlights::detect_highlights`)
///   against a football-highlight project produces these directly; nothing
///   sport-specific to add to `Template` for it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct SportsOverlaySettings {
    /// UI-seed only (module doc comment) — no overlay renderer exists yet.
    pub score_overlay_suggested: bool,
    /// Real, Phase 11 §38 types: default role/ducking to seed onto a
    /// `Music`-role track ("duck music under commentary").
    pub music_role: AudioRole,
    pub music_ducking: DuckingSettings,
}

/// One reusable project/edit template (master prompt §36). Every field is a
/// reference to (or the exact type of) an existing, real settings type
/// elsewhere in this codebase — see the module doc comment for the mapping
/// and for the two fields that are honestly structural-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `true` for one of the 8 [`all_templates`] built-ins (stable literal
    /// id, never user-deletable — `commands::templates::delete_custom_template`
    /// refuses); `false` for a user-saved/imported custom template.
    pub is_built_in: bool,
    pub canvas: CanvasV1,
    pub caption_style: CaptionStyle,
    /// Zoom "settings" (master prompt §36): `zoom::ZoomIntensity` is the one
    /// thing Phase 11's auto-zoom module exposes as a persisted, user-facing
    /// knob — trigger detection itself (scene/marker/emphasis) runs against
    /// a project's actual media/transcript at generation time, not against
    /// template-stored data, so there is nothing else "zoom setting"-shaped
    /// to store here.
    pub zoom_intensity: ZoomIntensity,
    /// Silence-removal padding/merge (master prompt §36 "silence settings"),
    /// the exact `vad::cutlist::CutParams` Phase 5 already uses.
    pub silence_settings: CutParams,
    pub transition_settings: TransitionSettings,
    /// References one of `render::presets::all_presets()`'s stable ids —
    /// never a duplicated copy of `RenderSettings` (module doc comment).
    pub export_preset_id: String,
    pub ai_prompt_config: AiPromptConfig,
    /// `Some` only for the Football Highlight built-in (or a custom template
    /// saved from one) — master prompt §37's sport-specific extras. `None`
    /// for every other template. Logo overlay (§37's "optional... generic"
    /// slot) is deliberately NOT a field here: it is never a bundled asset
    /// (§37 "do NOT depend on proprietary assets"), so there is nothing
    /// this catalog itself should store — a caller who wants a logo
    /// attaches an existing `MediaItem::id` from their own project's media
    /// library at build/apply time, by convention, not through this schema.
    pub sports_overlay: Option<SportsOverlaySettings>,

    // -- Upgrade spec §3/§17: asset-by-id references ------------------------
    /// `#[serde(default)]` so every custom template saved before this field
    /// existed still deserializes, as `None` — upgrade spec §17's own
    /// asset-by-id requirement, validated against the Asset Library at
    /// save/update time (see [`validate_asset_references`]), never a raw
    /// path.
    #[serde(default)]
    pub intro: Option<AssetReference>,
    #[serde(default)]
    pub outro: Option<AssetReference>,
    #[serde(default)]
    pub watermark: Option<WatermarkReference>,
    #[serde(default)]
    pub background_music: Option<BackgroundMusicReference>,

    // -- Upgrade spec §20: versioning ----------------------------------------
    /// Starts at `1` for every built-in (immutable — never bumped, see
    /// [`TemplateError::CannotEditBuiltIn`]) and for a brand-new custom
    /// template; increments by 1 on every subsequent
    /// `commands::templates::update_custom_template` call that saves over
    /// this same custom template id. `#[serde(default = "default_template_version")]`
    /// so a template JSON saved before versioning existed deserializes as
    /// version `1` — the sensible "this is the only version that ever
    /// existed" reading, not a made-up placeholder.
    #[serde(default = "default_template_version")]
    pub version: u32,
}

fn canvas_16x9() -> CanvasV1 {
    CanvasV1::default() // 1920x1080, Ratio16x9 — see project::types::CanvasV1's own Default impl.
}

fn canvas_9x16() -> CanvasV1 {
    CanvasV1 {
        width: 1080,
        height: 1920,
        fps: Rational::new(30, 1),
        ratio_preset: CanvasRatioPreset::Ratio9x16,
    }
}

/// Looks up a built-in caption style by id (module doc comment: templates
/// reference the *existing* Phase 8 catalog rather than inventing
/// near-duplicate style objects). Only ever called here with the built-in
/// catalog's own literal ids, so a miss means this module itself has a
/// broken cross-reference — panics rather than silently producing a
/// half-built `Template`, and is caught immediately by
/// `every_templates_caption_style_and_export_preset_id_cross_references_a_real_catalog_entry`
/// below.
fn caption_style(id: &str) -> CaptionStyle {
    styles::all_caption_templates()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("templates: no caption style with id {id:?}"))
}

fn talking_head() -> Template {
    Template {
        id: "tmpl_talking_head".to_string(),
        name: "Talking Head".to_string(),
        description: "Single-speaker, face-to-camera content: clean unobtrusive captions, a \
            gentle push-in on emphasis, and generous silence padding so natural pauses survive."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_minimal"),
        zoom_intensity: ZoomIntensity::Medium,
        silence_settings: CutParams {
            padding_before_us: 200_000,
            padding_after_us: 200_000,
            merge_gap_us: 300_000,
        },
        transition_settings: TransitionSettings::cut(),
        export_preset_id: "p1080".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::WeakSentence,
                SmartEditCategory::FillerWord,
                SmartEditCategory::LongPause,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn podcast() -> Template {
    Template {
        id: "tmpl_podcast".to_string(),
        name: "Podcast".to_string(),
        description: "Long-form conversational audio/video: understated lower-third captions, \
            minimal zoom, and aggressive merging of nearby thinking-pauses."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_podcast"),
        zoom_intensity: ZoomIntensity::Low,
        silence_settings: CutParams {
            padding_before_us: 150_000,
            padding_after_us: 150_000,
            merge_gap_us: 500_000,
        },
        transition_settings: TransitionSettings::cut(),
        export_preset_id: "p1080".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::FillerWord,
                SmartEditCategory::LongPause,
                SmartEditCategory::Repetition,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn tiktok() -> Template {
    Template {
        id: "tmpl_tiktok".to_string(),
        name: "TikTok".to_string(),
        description: "Vertical short-form: bold high-contrast captions, strong zoom punches, \
            tight silence removal, and a quick cross-fade between clips."
            .to_string(),
        is_built_in: true,
        canvas: canvas_9x16(),
        caption_style: caption_style("template_tiktok"),
        zoom_intensity: ZoomIntensity::High,
        silence_settings: CutParams {
            padding_before_us: 80_000,
            padding_after_us: 80_000,
            merge_gap_us: 150_000,
        },
        transition_settings: TransitionSettings::cross_fade(150_000),
        export_preset_id: "tiktok_1080x1920".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::BoringSection,
                SmartEditCategory::WeakSentence,
                SmartEditCategory::OffTopic,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn youtube_shorts() -> Template {
    Template {
        id: "tmpl_youtube_shorts".to_string(),
        name: "YouTube Shorts".to_string(),
        description: "Vertical short-form for YouTube: word-highlight (karaoke) captions, \
            moderate zoom, and a slightly slower cross-fade than TikTok's."
            .to_string(),
        is_built_in: true,
        canvas: canvas_9x16(),
        // Word-highlight captions read well for Shorts and deliberately
        // differ from TikTok's own template above, per master prompt §26's
        // "Karaoke" style's own doc comment (active-word highlighting).
        caption_style: caption_style("template_karaoke"),
        zoom_intensity: ZoomIntensity::Medium,
        silence_settings: CutParams {
            padding_before_us: 100_000,
            padding_after_us: 100_000,
            merge_gap_us: 200_000,
        },
        transition_settings: TransitionSettings::cross_fade(200_000),
        // No dedicated "YouTube Shorts" render preset exists in
        // `render::presets::all_presets()` (only one vertical preset,
        // `tiktok_1080x1920`, does — same 1080x1920 target resolution
        // Shorts also uses) — reused honestly rather than inventing a
        // near-duplicate preset for the same resolution/bitrate.
        export_preset_id: "tiktok_1080x1920".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::Repetition,
                SmartEditCategory::DuplicateIdea,
                SmartEditCategory::BoringSection,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn news() -> Template {
    Template {
        id: "tmpl_news".to_string(),
        name: "News".to_string(),
        description: "Broadcast-style delivery: solid lower-third captions, no zoom, and \
            conservative silence removal that preserves natural broadcast pacing."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_news"),
        zoom_intensity: ZoomIntensity::Off,
        silence_settings: CutParams {
            padding_before_us: 400_000,
            padding_after_us: 400_000,
            merge_gap_us: 800_000,
        },
        transition_settings: TransitionSettings::cut(),
        export_preset_id: "p1080".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::OffTopic,
                SmartEditCategory::WeakSentence,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn tutorial() -> Template {
    Template {
        id: "tmpl_tutorial".to_string(),
        name: "Tutorial".to_string(),
        description: "Screen-recording/how-to content: clean unobtrusive captions that stay out \
            of the way of on-screen UI, a moderate zoom on key steps, and aggressive merging of \
            the long pauses that come from typing/switching windows."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_minimal"),
        zoom_intensity: ZoomIntensity::Medium,
        silence_settings: CutParams {
            padding_before_us: 150_000,
            padding_after_us: 150_000,
            merge_gap_us: 600_000,
        },
        transition_settings: TransitionSettings::cut(),
        export_preset_id: "p1080".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::LongPause,
                SmartEditCategory::FillerWord,
                SmartEditCategory::UnnecessaryIntro,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

fn gaming() -> Template {
    Template {
        id: "tmpl_gaming".to_string(),
        name: "Gaming".to_string(),
        description: "Gaming/streaming highlights: vivid thick-outlined captions, strong zoom \
            punches, tight cuts, and a quick cross-fade between clips."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_gaming"),
        zoom_intensity: ZoomIntensity::High,
        silence_settings: CutParams {
            padding_before_us: 100_000,
            padding_after_us: 100_000,
            merge_gap_us: 200_000,
        },
        transition_settings: TransitionSettings::cross_fade(150_000),
        // Bitrate-controlled (not CRF) at a rate matching YouTube's own
        // recommended 1080p30 upload bitrate — long-form gaming uploads are
        // conventionally shared to YouTube, unlike this catalog's other
        // `p1080` (CRF-controlled) users.
        export_preset_id: "youtube_1080p".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::BoringSection,
                SmartEditCategory::LongPause,
                SmartEditCategory::Repetition,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: None,
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

/// Master prompt §37: 16:9 by default (still fully selectable at 9:16 by
/// overriding `canvas` on the produced `Template`/project — this catalog
/// entry's own `canvas` field is just the sensible starting point, exactly
/// like every other preset/template in this codebase is "a starting point,
/// not an all-or-nothing choice", per `render::presets`' own module doc
/// comment), high-energy captions (reusing the Gaming caption style — bold,
/// thick-outlined, vivid — rather than inventing a near-duplicate), fast
/// (short-duration) cross-fade transitions, and a `sports_overlay` carrying
/// this template's real §38 music-ducking defaults. See [`SportsOverlaySettings`]'s
/// doc comment for how the remaining §37 bullets (highlight/replay markers,
/// slow-motion) map onto already-real mechanisms with no new field needed.
fn football_highlight() -> Template {
    Template {
        id: "tmpl_football_highlight".to_string(),
        name: "Football Highlight".to_string(),
        description: "Generic sports highlight reel (§37 — no proprietary team/league assets): \
            16:9 by default (also usable at 9:16), high-energy captions, the fastest cross-fade \
            transitions in this catalog, and a ducked music-track default under commentary."
            .to_string(),
        is_built_in: true,
        canvas: canvas_16x9(),
        caption_style: caption_style("template_gaming"),
        zoom_intensity: ZoomIntensity::High,
        silence_settings: CutParams {
            padding_before_us: 50_000,
            padding_after_us: 50_000,
            merge_gap_us: 100_000,
        },
        // Fastest transition duration in the whole catalog (§37 "fast
        // transitions") — shorter than TikTok's/Gaming's 150ms and YouTube
        // Shorts' 200ms.
        transition_settings: TransitionSettings::cross_fade(100_000),
        export_preset_id: "youtube_1080p".to_string(),
        ai_prompt_config: AiPromptConfig {
            emphasized_categories: vec![
                SmartEditCategory::BoringSection,
                SmartEditCategory::LongPause,
            ],
            system_prompt_prefix: None,
        },
        sports_overlay: Some(SportsOverlaySettings {
            score_overlay_suggested: true,
            music_role: AudioRole::Music,
            music_ducking: DuckingSettings {
                duck_level: 0.25,
                attack_us: 150_000,
                release_us: 400_000,
            },
        }),
        intro: None,
        outro: None,
        watermark: None,
        background_music: None,
        version: default_template_version(),
    }
}

/// The 8 built-in templates (master prompt §36's exact list), in the order
/// §36 lists them. Pure catalog function, same pattern as
/// `render::presets::all_presets`/`captions::styles::all_caption_templates` —
/// every call returns fresh owned values with fixed, stable `id`s.
pub fn all_templates() -> Vec<Template> {
    vec![
        talking_head(),
        podcast(),
        tiktok(),
        youtube_shorts(),
        news(),
        tutorial(),
        gaming(),
        football_highlight(),
    ]
}

/// Input to [`save_as_template_from_project`]: the caller supplies whatever
/// real settings values are "current" for their editing session. Most of
/// these fields (`zoom_intensity`, `silence_settings`, `transition_settings`,
/// `export_preset_id`, `ai_prompt_config`) are one-shot command parameters
/// elsewhere in this codebase, not fields persisted on `ProjectV1` itself
/// (auto-zoom/cutlist/render presets are all re-supplied per invocation, per
/// `zoom`/`vad::cutlist`/`render::presets`'s own doc comments — there is no
/// project-wide "current setting" to read them off of). `canvas` and
/// `caption_style_id` ARE real persisted `ProjectV1` state
/// (`ProjectV1::canvas`, `ProjectV1::caption_styles`) and are the two
/// settings [`save_as_template_from_project`] reads directly from the given
/// project rather than asking the caller to repeat them.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SaveAsTemplateInput {
    pub name: String,
    pub description: String,
    /// Looked up first in `project.caption_styles`, falling back to the
    /// built-in catalog (`captions::styles::all_caption_templates`) if not
    /// found there — so "Save as Template" works whether the project's
    /// current caption style is one of its own custom styles or a built-in
    /// one it never copied into `caption_styles`.
    pub caption_style_id: String,
    pub zoom_intensity: ZoomIntensity,
    pub silence_settings: CutParams,
    pub transition_settings: TransitionSettings,
    pub export_preset_id: String,
    pub ai_prompt_config: AiPromptConfig,
    pub sports_overlay: Option<SportsOverlaySettings>,

    // -- Upgrade spec §3/§17: asset-by-id references, validated against the
    //    caller's Asset Library before ever landing on the produced
    //    `Template` (see `validate_asset_references`). `#[serde(default)]`
    //    so a frontend/older caller that doesn't send these yet still
    //    deserializes cleanly as "none set".
    #[serde(default)]
    pub intro: Option<AssetReference>,
    #[serde(default)]
    pub outro: Option<AssetReference>,
    #[serde(default)]
    pub watermark: Option<WatermarkReference>,
    #[serde(default)]
    pub background_music: Option<BackgroundMusicReference>,
}

/// Upgrade spec §17: validates that every asset id a `Template` would
/// reference (`intro`/`outro`/`watermark`/`background_music`) actually
/// exists in the caller's Asset Library — `known_asset_ids` is the set of
/// ids `commands::templates` reads from `assets::io::list_assets` at
/// save/update time. A pure function (no filesystem access itself) so it's
/// directly unit-testable against a synthetic `HashSet`, same separation of
/// concerns `save_as_template_from_project` already keeps between "pure
/// validation" and "the command layer's own I/O".
fn validate_asset_references(
    intro: Option<&AssetReference>,
    outro: Option<&AssetReference>,
    watermark: Option<&WatermarkReference>,
    background_music: Option<&BackgroundMusicReference>,
    known_asset_ids: &HashSet<String>,
) -> Result<(), TemplateError> {
    let check = |asset_id: &str| -> Result<(), TemplateError> {
        if known_asset_ids.contains(asset_id) {
            Ok(())
        } else {
            Err(TemplateError::UnknownAsset {
                asset_id: asset_id.to_string(),
            })
        }
    };
    if let Some(r) = intro {
        check(&r.asset_id)?;
    }
    if let Some(r) = outro {
        check(&r.asset_id)?;
    }
    if let Some(r) = watermark {
        check(&r.asset_id)?;
    }
    if let Some(r) = background_music {
        check(&r.asset_id)?;
    }
    Ok(())
}

/// Shared builder behind both [`save_as_template_from_project`] (a brand
/// new custom template) and [`update_custom_template`] (overwriting an
/// existing one) — same caption-style/export-preset/asset-reference
/// validation either way, differing only in `id`/`version`.
fn build_custom_template(
    id: String,
    version: u32,
    project: &ProjectV1,
    input: SaveAsTemplateInput,
    known_asset_ids: &HashSet<String>,
) -> Result<Template, TemplateError> {
    let caption_style = project
        .caption_styles
        .iter()
        .find(|s| s.id == input.caption_style_id)
        .cloned()
        .or_else(|| {
            styles::all_caption_templates()
                .into_iter()
                .find(|s| s.id == input.caption_style_id)
        })
        .ok_or_else(|| TemplateError::UnknownCaptionStyle {
            style_id: input.caption_style_id.clone(),
        })?;

    find_preset(&input.export_preset_id).map_err(|_| TemplateError::UnknownExportPreset {
        preset_id: input.export_preset_id.clone(),
    })?;

    validate_asset_references(
        input.intro.as_ref(),
        input.outro.as_ref(),
        input.watermark.as_ref(),
        input.background_music.as_ref(),
        known_asset_ids,
    )?;

    Ok(Template {
        id,
        name: input.name,
        description: input.description,
        is_built_in: false,
        canvas: project.canvas.clone(),
        caption_style,
        zoom_intensity: input.zoom_intensity,
        silence_settings: input.silence_settings,
        transition_settings: input.transition_settings,
        export_preset_id: input.export_preset_id,
        ai_prompt_config: input.ai_prompt_config,
        sports_overlay: input.sports_overlay,
        intro: input.intro,
        outro: input.outro,
        watermark: input.watermark,
        background_music: input.background_music,
        version,
    })
}

/// Save as Template (master prompt §36): snapshots the given project's
/// `canvas` plus the caller-supplied "current" settings bundle into a new
/// custom `Template` at version 1. Validates `caption_style_id`/
/// `export_preset_id` against real catalogs and any `intro`/`outro`/
/// `watermark`/`background_music` asset id against `known_asset_ids`
/// (never silently produces a `Template` with a broken cross-reference).
pub fn save_as_template_from_project(
    project: &ProjectV1,
    input: SaveAsTemplateInput,
    known_asset_ids: &HashSet<String>,
) -> Result<Template, TemplateError> {
    build_custom_template(
        format!("custom_{}", uuid::Uuid::new_v4()),
        default_template_version(),
        project,
        input,
        known_asset_ids,
    )
}

/// Update an existing custom template in place (upgrade spec §20): same id,
/// version bumped to `existing.version + 1`. Refuses to edit a built-in
/// (`TemplateError::CannotEditBuiltIn`) — the caller
/// (`commands::templates::update_custom_template`) is responsible for
/// persisting `existing`'s pre-update content to the version-history store
/// before ever calling this, so an older `template_id` + `template_version`
/// pin (§20's own requirement) stays resolvable afterwards.
pub fn update_custom_template(
    existing: &Template,
    project: &ProjectV1,
    input: SaveAsTemplateInput,
    known_asset_ids: &HashSet<String>,
) -> Result<Template, TemplateError> {
    if existing.is_built_in {
        return Err(TemplateError::CannotEditBuiltIn {
            template_id: existing.id.clone(),
        });
    }
    build_custom_template(
        existing.id.clone(),
        existing.version + 1,
        project,
        input,
        known_asset_ids,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::all_presets;

    #[test]
    fn all_eight_built_in_templates_are_present_exactly_once() {
        let templates = all_templates();
        assert_eq!(templates.len(), 8);
        let ids: HashSet<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 8, "template ids must be unique");
        for expected in [
            "tmpl_talking_head",
            "tmpl_podcast",
            "tmpl_tiktok",
            "tmpl_youtube_shorts",
            "tmpl_news",
            "tmpl_tutorial",
            "tmpl_gaming",
            "tmpl_football_highlight",
        ] {
            assert!(ids.contains(expected), "missing template {expected}");
        }
        let names: HashSet<&str> = templates.iter().map(|t| t.name.as_str()).collect();
        for expected in [
            "Talking Head",
            "Podcast",
            "TikTok",
            "YouTube Shorts",
            "News",
            "Tutorial",
            "Gaming",
            "Football Highlight",
        ] {
            assert!(
                names.contains(expected),
                "missing template named {expected}"
            );
        }
        assert!(templates.iter().all(|t| t.is_built_in));
    }

    #[test]
    fn every_built_in_has_sane_non_empty_metadata() {
        for t in all_templates() {
            assert!(!t.id.is_empty());
            assert!(!t.name.is_empty());
            assert!(!t.description.is_empty());
        }
    }

    #[test]
    fn every_template_caption_style_and_export_preset_id_cross_reference_a_real_catalog_entry() {
        let valid_style_ids: HashSet<String> = styles::all_caption_templates()
            .into_iter()
            .map(|s| s.id)
            .collect();
        let valid_preset_ids: HashSet<&'static str> =
            all_presets().into_iter().map(|p| p.id).collect();

        for t in all_templates() {
            assert!(
                valid_style_ids.contains(&t.caption_style.id),
                "{}: caption_style.id {:?} is not in captions::styles::all_caption_templates()",
                t.id,
                t.caption_style.id
            );
            assert!(
                valid_preset_ids.contains(t.export_preset_id.as_str()),
                "{}: export_preset_id {:?} is not in render::presets::all_presets()",
                t.id,
                t.export_preset_id
            );
            // Also exercise the real lookup function, not just id-set membership.
            assert!(find_preset(&t.export_preset_id).is_ok());
        }
    }

    #[test]
    fn football_highlight_is_16x9_by_default() {
        let t = all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_football_highlight")
            .unwrap();
        assert_eq!(t.canvas.ratio_preset, CanvasRatioPreset::Ratio16x9);
        assert_eq!((t.canvas.width, t.canvas.height), (1920, 1080));
    }

    #[test]
    fn tiktok_and_youtube_shorts_are_9x16() {
        for id in ["tmpl_tiktok", "tmpl_youtube_shorts"] {
            let t = all_templates().into_iter().find(|t| t.id == id).unwrap();
            assert_eq!(t.canvas.ratio_preset, CanvasRatioPreset::Ratio9x16, "{id}");
            assert_eq!((t.canvas.width, t.canvas.height), (1080, 1920), "{id}");
        }
    }

    #[test]
    fn football_highlight_carries_real_sports_overlay_music_ducking_defaults() {
        let t = all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_football_highlight")
            .unwrap();
        let overlay = t
            .sports_overlay
            .expect("football highlight has sports_overlay");
        assert_eq!(overlay.music_role, AudioRole::Music);
        assert!(overlay.music_ducking.duck_level < 1.0);
        assert!(overlay.score_overlay_suggested);
    }

    #[test]
    fn every_non_football_template_has_no_sports_overlay() {
        for t in all_templates() {
            if t.id == "tmpl_football_highlight" {
                continue;
            }
            assert!(
                t.sports_overlay.is_none(),
                "{} should have no sports_overlay",
                t.id
            );
        }
    }

    #[test]
    fn football_highlight_has_the_fastest_transition_in_the_catalog() {
        let templates = all_templates();
        let football = templates
            .iter()
            .find(|t| t.id == "tmpl_football_highlight")
            .unwrap();
        assert_eq!(
            football.transition_settings.transition_type,
            TransitionType::CrossFade
        );
        for other in &templates {
            if other.id == football.id {
                continue;
            }
            if other.transition_settings.transition_type == TransitionType::CrossFade {
                assert!(
                    football.transition_settings.duration_us
                        < other.transition_settings.duration_us,
                    "{} should be strictly faster than {}",
                    football.id,
                    other.id
                );
            }
        }
    }

    #[test]
    fn templates_are_meaningfully_differentiated_not_eight_copies_of_the_same_defaults() {
        let templates = all_templates();
        let zoom_levels: HashSet<ZoomIntensity> =
            templates.iter().map(|t| t.zoom_intensity).collect();
        assert!(
            zoom_levels.len() > 1,
            "zoom intensity should vary across templates"
        );

        let silence_settings: HashSet<(i64, i64, i64)> = templates
            .iter()
            .map(|t| {
                (
                    t.silence_settings.padding_before_us,
                    t.silence_settings.padding_after_us,
                    t.silence_settings.merge_gap_us,
                )
            })
            .collect();
        assert!(
            silence_settings.len() > 1,
            "silence settings should vary across templates"
        );

        let caption_style_ids: HashSet<&str> = templates
            .iter()
            .map(|t| t.caption_style.id.as_str())
            .collect();
        assert!(
            caption_style_ids.len() > 1,
            "caption styles should vary across templates"
        );

        let export_preset_ids: HashSet<&str> = templates
            .iter()
            .map(|t| t.export_preset_id.as_str())
            .collect();
        assert!(
            export_preset_ids.len() > 1,
            "export presets should vary across templates"
        );

        let canvases: HashSet<(u32, u32)> = templates
            .iter()
            .map(|t| (t.canvas.width, t.canvas.height))
            .collect();
        assert!(
            canvases.len() > 1,
            "canvas dimensions should vary across templates"
        );
    }

    // -- save_as_template_from_project ---------------------------------------

    fn sample_project() -> ProjectV1 {
        let mut project = ProjectV1::new("Save-As-Template Test");
        project.canvas = canvas_9x16();
        project
            .caption_styles
            .push(caption_style("template_karaoke"));
        project
    }

    fn sample_input(caption_style_id: &str, export_preset_id: &str) -> SaveAsTemplateInput {
        SaveAsTemplateInput {
            name: "My Custom Template".to_string(),
            description: "A saved custom template".to_string(),
            caption_style_id: caption_style_id.to_string(),
            zoom_intensity: ZoomIntensity::High,
            silence_settings: CutParams {
                padding_before_us: 42,
                padding_after_us: 43,
                merge_gap_us: 44,
            },
            transition_settings: TransitionSettings::cross_fade(90_000),
            export_preset_id: export_preset_id.to_string(),
            ai_prompt_config: AiPromptConfig {
                emphasized_categories: vec![SmartEditCategory::Repetition],
                system_prompt_prefix: Some("Be extra aggressive.".to_string()),
            },
            sports_overlay: None,
            intro: None,
            outro: None,
            watermark: None,
            background_music: None,
        }
    }

    #[test]
    fn save_as_template_captures_the_projects_canvas_and_the_callers_current_settings() {
        let project = sample_project();
        let input = sample_input("template_karaoke", "p1080");
        let template = save_as_template_from_project(&project, input, &HashSet::new())
            .expect("save_as_template");

        assert!(!template.is_built_in);
        assert!(template.id.starts_with("custom_"));
        assert_eq!(template.name, "My Custom Template");
        assert_eq!(template.canvas, project.canvas);
        assert_eq!(template.caption_style.id, "template_karaoke");
        assert_eq!(template.zoom_intensity, ZoomIntensity::High);
        assert_eq!(template.silence_settings.padding_before_us, 42);
        assert_eq!(template.transition_settings.duration_us, 90_000);
        assert_eq!(template.export_preset_id, "p1080");
        assert_eq!(
            template.ai_prompt_config.emphasized_categories,
            vec![SmartEditCategory::Repetition]
        );
        assert_eq!(template.version, 1);
        assert!(template.intro.is_none());
        assert!(template.watermark.is_none());
    }

    #[test]
    fn save_as_template_finds_a_caption_style_from_the_built_in_catalog_when_not_on_the_project() {
        // The project's own caption_styles list only has "template_karaoke"
        // (see sample_project) — asking for a different, built-in-only style
        // id must still resolve via the fallback catalog lookup.
        let project = sample_project();
        let input = sample_input("template_news", "p1080");
        let template = save_as_template_from_project(&project, input, &HashSet::new())
            .expect("save_as_template");
        assert_eq!(template.caption_style.id, "template_news");
    }

    #[test]
    fn save_as_template_errors_on_an_unknown_caption_style_id() {
        let project = sample_project();
        let input = sample_input("does_not_exist", "p1080");
        let err = save_as_template_from_project(&project, input, &HashSet::new()).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownCaptionStyle { .. }));
    }

    #[test]
    fn save_as_template_errors_on_an_unknown_export_preset_id() {
        let project = sample_project();
        let input = sample_input("template_karaoke", "does_not_exist");
        let err = save_as_template_from_project(&project, input, &HashSet::new()).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownExportPreset { .. }));
    }

    // -- asset-by-id references (upgrade spec §17) ---------------------------

    #[test]
    fn save_as_template_accepts_asset_references_that_exist_in_the_library() {
        let project = sample_project();
        let mut input = sample_input("template_karaoke", "p1080");
        input.intro = Some(AssetReference {
            asset_id: "asset_intro_1".to_string(),
        });
        input.watermark = Some(WatermarkReference {
            asset_id: "asset_logo_1".to_string(),
            position: WatermarkPosition::BottomRight,
        });
        input.background_music = Some(BackgroundMusicReference {
            asset_id: "asset_music_1".to_string(),
            volume: 0.2,
        });
        let known: HashSet<String> = ["asset_intro_1", "asset_logo_1", "asset_music_1"]
            .into_iter()
            .map(String::from)
            .collect();

        let template =
            save_as_template_from_project(&project, input, &known).expect("save_as_template");
        assert_eq!(
            template.intro.unwrap().asset_id,
            "asset_intro_1".to_string()
        );
        assert_eq!(
            template.watermark.as_ref().unwrap().position,
            WatermarkPosition::BottomRight
        );
        assert_eq!(template.background_music.unwrap().volume, 0.2);
    }

    #[test]
    fn save_as_template_errors_on_an_unknown_asset_id() {
        let project = sample_project();
        let mut input = sample_input("template_karaoke", "p1080");
        input.intro = Some(AssetReference {
            asset_id: "does_not_exist".to_string(),
        });
        let err = save_as_template_from_project(&project, input, &HashSet::new()).unwrap_err();
        assert!(matches!(
            err,
            TemplateError::UnknownAsset { asset_id } if asset_id == "does_not_exist"
        ));
    }

    // -- versioning (upgrade spec §20) ----------------------------------------

    #[test]
    fn every_built_in_template_starts_at_version_1() {
        for t in all_templates() {
            assert_eq!(t.version, 1, "{} should start at version 1", t.id);
        }
    }

    #[test]
    fn update_custom_template_increments_the_version_on_each_call() {
        let project = sample_project();
        let v1 = save_as_template_from_project(
            &project,
            sample_input("template_karaoke", "p1080"),
            &HashSet::new(),
        )
        .expect("v1");
        assert_eq!(v1.version, 1);

        let v2 = update_custom_template(
            &v1,
            &project,
            sample_input("template_karaoke", "p1080"),
            &HashSet::new(),
        )
        .expect("v2");
        assert_eq!(v2.version, 2);
        assert_eq!(v2.id, v1.id, "the id must stay stable across an update");

        let v3 = update_custom_template(
            &v2,
            &project,
            sample_input("template_karaoke", "p1080"),
            &HashSet::new(),
        )
        .expect("v3");
        assert_eq!(v3.version, 3);
        assert_eq!(v3.id, v1.id);
    }

    #[test]
    fn update_custom_template_refuses_to_edit_a_built_in() {
        let project = sample_project();
        let built_in = all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_tiktok")
            .unwrap();
        let err = update_custom_template(
            &built_in,
            &project,
            sample_input("template_karaoke", "p1080"),
            &HashSet::new(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TemplateError::CannotEditBuiltIn { template_id } if template_id == "tmpl_tiktok"
        ));
    }

    #[test]
    fn update_custom_template_also_validates_asset_references() {
        let project = sample_project();
        let v1 = save_as_template_from_project(
            &project,
            sample_input("template_karaoke", "p1080"),
            &HashSet::new(),
        )
        .expect("v1");

        let mut bad_input = sample_input("template_karaoke", "p1080");
        bad_input.outro = Some(AssetReference {
            asset_id: "does_not_exist".to_string(),
        });
        let err = update_custom_template(&v1, &project, bad_input, &HashSet::new()).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownAsset { .. }));
    }

    // -- backward-compatible deserialization ----------------------------------

    /// An OLD-shaped custom template JSON, saved before this upgrade added
    /// `intro`/`outro`/`watermark`/`background_music`/`version` — must still
    /// deserialize, with the new fields defaulting sensibly (`None` for the
    /// references, `1` for `version`, per each field's own
    /// `#[serde(default...)]` attribute), not fail or silently drop data.
    #[test]
    fn an_old_template_json_without_the_new_fields_deserializes_with_sensible_defaults() {
        let old_json = serde_json::json!({
            "id": "custom_old_one",
            "name": "Old Template",
            "description": "Saved before versioning/asset-references existed",
            "is_built_in": false,
            "canvas": canvas_9x16(),
            "caption_style": caption_style("template_karaoke"),
            "zoom_intensity": "high",
            "silence_settings": {
                "padding_before_us": 1,
                "padding_after_us": 2,
                "merge_gap_us": 3
            },
            "transition_settings": { "transition_type": "cut", "duration_us": 0 },
            "export_preset_id": "p1080",
            "ai_prompt_config": { "emphasized_categories": [], "system_prompt_prefix": null },
            "sports_overlay": null
            // Deliberately no "intro"/"outro"/"watermark"/"background_music"/"version".
        });

        let template: Template =
            serde_json::from_value(old_json).expect("old-shaped template JSON must still parse");
        assert_eq!(template.version, 1);
        assert!(template.intro.is_none());
        assert!(template.outro.is_none());
        assert!(template.watermark.is_none());
        assert!(template.background_music.is_none());
    }
}
