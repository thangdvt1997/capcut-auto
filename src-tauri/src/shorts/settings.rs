//! `ShortsSettings` — the three user-facing settings master prompt §22 lists
//! for the Long-Video-to-Shorts pipeline: duration, aspect, and clip count.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{CanvasRatioPreset, Rational};

/// Target duration for a generated short (master prompt §22: "15s / 30s /
/// 60s / 90s / custom"). A closed enum for the four fixed presets plus one
/// explicit `Custom` variant — never a bare `u32` seconds field, so a caller
/// can't accidentally pass an unintended value where one of the four named
/// presets was meant (the same "closed, not stringly/numerically typed"
/// discipline `ai::edit_plan::EditOperation`/`highlights` use throughout this
/// codebase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DurationSetting {
    Fixed15,
    Fixed30,
    Fixed60,
    Fixed90,
    Custom { seconds: u32 },
}

impl DurationSetting {
    /// Target duration in microseconds, this schema's own timebase
    /// (`docs/architecture.md` "Timebase conversion boundaries" — `i64`
    /// microseconds everywhere in the core model).
    pub fn target_duration_us(self) -> i64 {
        let seconds = match self {
            DurationSetting::Fixed15 => 15,
            DurationSetting::Fixed30 => 30,
            DurationSetting::Fixed60 => 60,
            DurationSetting::Fixed90 => 90,
            DurationSetting::Custom { seconds } => seconds,
        };
        i64::from(seconds) * 1_000_000
    }
}

/// Target aspect ratio for a generated short (master prompt §22: "9:16 / 1:1
/// / 4:5" — TikTok/Shorts/Reels are generic labels over these three ratios,
/// per this pass's own task brief; no platform-specific export exists beyond
/// this).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShortsAspect {
    Vertical9x16,
    Square1x1,
    Portrait4x5,
}

impl ShortsAspect {
    /// A concrete 1080-scaled canvas size for this aspect — matching this
    /// codebase's existing `render::presets` convention of shipping a
    /// specific pixel resolution per named preset (e.g.
    /// `tiktok_1080x1920`) rather than leaving width/height to the caller to
    /// derive from a bare ratio.
    pub fn canvas_dimensions(self) -> (u32, u32) {
        match self {
            ShortsAspect::Vertical9x16 => (1080, 1920),
            ShortsAspect::Square1x1 => (1080, 1080),
            ShortsAspect::Portrait4x5 => (1080, 1350),
        }
    }

    /// The matching `CanvasRatioPreset` — `ProjectV1::canvas` already has a
    /// closed ratio-preset enum for exactly this purpose, so a generated
    /// short's canvas is tagged the same way any other project's canvas is,
    /// never left as `Custom` when a named preset applies.
    pub fn ratio_preset(self) -> CanvasRatioPreset {
        match self {
            ShortsAspect::Vertical9x16 => CanvasRatioPreset::Ratio9x16,
            ShortsAspect::Square1x1 => CanvasRatioPreset::Ratio1x1,
            ShortsAspect::Portrait4x5 => CanvasRatioPreset::Ratio4x5,
        }
    }
}

/// Standard 30fps for every generated short — the source media's own frame
/// rate is a per-file detail this pipeline doesn't need to inherit exactly;
/// 30fps matches `CanvasV1`'s own "keep the default project simple" Phase 2
/// precedent (`project::types::CanvasV1` doc comment).
pub const SHORT_CANVAS_FPS: Rational = Rational::new(30, 1);

/// Settings surface for `commands::shorts::generate_shorts` (master prompt
/// §22's exact three settings groups).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ShortsSettings {
    pub duration: DurationSetting,
    pub aspect: ShortsAspect,
    /// Master prompt §22 lists four specific choices (1/3/5/10) as the UI's
    /// own preset buttons, but nothing about the pipeline itself requires
    /// exactly those four values — `select_top_non_overlapping`
    /// (`shorts::ranking`) works correctly for any positive count. This
    /// field therefore accepts **any** positive `u32` (validated by the
    /// pipeline as ">= 1", not "one of exactly these four"), so a future
    /// custom-count UI control needs no backend change; the frontend is free
    /// to only ever expose the four named buttons.
    pub clip_count: u32,
}
