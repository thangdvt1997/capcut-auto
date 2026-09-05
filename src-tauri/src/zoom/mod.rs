//! Auto-Zoom (master prompt §24): intelligent, keyframe-based zoom
//! triggered by a long static scene (reusing this phase's own real
//! `media::scene::Scene` detection), manual markers (caller-supplied
//! timestamps), or emphasized speech (reusing Phase 10's real RMS-energy
//! signal, `highlights::signals::windowed_rms_energy`, as a genuine,
//! computable proxy for "important sentence"/"speaker emphasis" — not a
//! fabricated importance score). Produces real `project::Keyframe` entries
//! (`property: "scale"`), reusing the existing schema rather than inventing
//! a parallel zoom-specific data shape; `timeline::zoom` wires the result
//! into a real clip's `ProjectV1::keyframes` through the standard
//! `Command`/undo machinery.
//!
//! ## Intensity -> peak scale (documented, since master prompt §24 gives one
//! worked example — `1.0 -> 1.08 -> 1.0 -> 1.12` — but not a table per level)
//!
//! - `Off`: no zoom keyframes are ever generated (`generate_zoom_keyframes`
//!   short-circuits before even looking at triggers).
//! - `Low`: peak scale `1.05` — a barely-perceptible 5% push-in, for a
//!   conservative setting.
//! - `Medium`: peak scale `1.08` — the master prompt's own worked example's
//!   first push value; the "normal" amount.
//! - `High`: peak scale `1.15` — noticeably stronger, still well short of
//!   the doubling territory that would read as a visual effect rather than
//!   an editing choice ("avoid excessive zoom").
//!
//! Each trigger produces a `1.0 -> peak -> peak -> 1.0` keyframe quadruplet
//! at `[start, start+ramp, end-ramp, end]`, generalizing the master prompt's
//! worked example (a push in, a hold, a return) to N independent trigger
//! events instead of two hardcoded values.
//!
//! ## "Avoid excessive zoom" / overlap handling
//!
//! Triggers are sorted and merged ([`merge_triggers`]) when they overlap or
//! sit closer than [`MIN_GAP_US`] apart, so two nearby triggers never stack
//! into a discontinuous double-zoom.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::media::scene::Scene;
use crate::project::Keyframe;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ZoomIntensity {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl ZoomIntensity {
    /// Peak scale multiplier this intensity ramps up to — module doc
    /// comment documents the exact numbers/reasoning.
    pub fn peak_scale(self) -> f64 {
        match self {
            ZoomIntensity::Off => 1.0,
            ZoomIntensity::Low => 1.05,
            ZoomIntensity::Medium => 1.08,
            ZoomIntensity::High => 1.15,
        }
    }
}

/// One detected/manual zoom trigger event: a time range worth punching in
/// on, plus a human-readable reason (surfaced to the frontend so a user can
/// see *why* a given zoom keyframe exists before deciding to keep it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ZoomTrigger {
    pub start_us: i64,
    pub end_us: i64,
    pub reason: String,
}

/// A candidate window plus its own already-computed real RMS energy
/// (`highlights::signals::windowed_rms_energy`, `0.0..=1.0`) — the input
/// [`emphasis_triggers`] scores against [`EMPHASIS_ENERGY_THRESHOLD`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct EmphasisWindow {
    pub start_us: i64,
    pub end_us: i64,
    pub energy: f32,
}

/// How long the in/out ramp takes, at most (see [`clamp_ramp_us`] for how a
/// very short trigger shrinks this so keyframes never cross).
const RAMP_US: i64 = 400_000; // 0.4s — fast enough to read as a deliberate push, not a jarring jump-cut.

/// Triggers closer together than this are merged into one wider event
/// (module doc comment's "avoid excessive zoom" handling).
const MIN_GAP_US: i64 = 300_000;

/// A scene is "long static" (module doc comment's trigger) once it exceeds
/// this duration with no cut inside it.
pub const STATIC_SCENE_MIN_DURATION_US: i64 = 4_000_000;

/// An [`EmphasisWindow`] is "emphasized speech" (module doc comment's
/// "important sentence/speaker emphasis" proxy) once its real RMS energy
/// clears this threshold.
pub const EMPHASIS_ENERGY_THRESHOLD: f32 = 0.5;

/// Trigger detector: long static scenes (module doc comment). Reuses this
/// phase's own `Scene` detection directly rather than re-deriving "no cuts
/// for a while" from raw cut timestamps a second time.
pub fn static_scene_triggers(scenes: &[Scene]) -> Vec<ZoomTrigger> {
    scenes
        .iter()
        .filter(|s| (s.end_us - s.start_us) >= STATIC_SCENE_MIN_DURATION_US)
        .map(|s| ZoomTrigger {
            start_us: s.start_us,
            end_us: s.end_us,
            reason: format!(
                "Long static scene ({:.1}s with no cuts)",
                (s.end_us - s.start_us) as f64 / 1_000_000.0
            ),
        })
        .collect()
}

/// Trigger detector: manual markers (module doc comment) — each caller-given
/// timestamp becomes a short trigger window centered on it so the ramp math
/// below has room to work even for a single instant marker.
pub fn manual_marker_triggers(marker_timestamps_us: &[i64]) -> Vec<ZoomTrigger> {
    marker_timestamps_us
        .iter()
        .map(|&t| ZoomTrigger {
            start_us: (t - RAMP_US).max(0),
            end_us: t + RAMP_US,
            reason: "Manual zoom marker".to_string(),
        })
        .collect()
}

/// Trigger detector: emphasized speech (module doc comment's real-signal
/// proxy for "important sentence"/"speaker emphasis") — a window becomes a
/// trigger once its own already-computed real RMS energy clears
/// [`EMPHASIS_ENERGY_THRESHOLD`].
pub fn emphasis_triggers(windows: &[EmphasisWindow]) -> Vec<ZoomTrigger> {
    windows
        .iter()
        .filter(|w| w.energy >= EMPHASIS_ENERGY_THRESHOLD)
        .map(|w| ZoomTrigger {
            start_us: w.start_us,
            end_us: w.end_us,
            reason: format!("High speech emphasis (RMS energy {:.2})", w.energy),
        })
        .collect()
}

/// Sorts and merges overlapping/near triggers (module doc comment's overlap
/// handling) into the final, non-overlapping event list keyframing should
/// run against.
pub fn merge_triggers(triggers: &[ZoomTrigger]) -> Vec<ZoomTrigger> {
    let mut sorted: Vec<ZoomTrigger> = triggers.to_vec();
    sorted.sort_by_key(|t| t.start_us);
    let mut merged: Vec<ZoomTrigger> = Vec::with_capacity(sorted.len());
    for t in sorted {
        if let Some(last) = merged.last_mut() {
            if t.start_us - last.end_us < MIN_GAP_US {
                last.end_us = last.end_us.max(t.end_us);
                last.reason = format!("{}; {}", last.reason, t.reason);
                continue;
            }
        }
        merged.push(t);
    }
    merged
}

/// A ramp on each side plus a plateau in between needs at least `2 * ramp`
/// inside the event; if the event itself is shorter than that, shrink the
/// ramp so the two middle keyframes never cross (module doc comment).
fn clamp_ramp_us(event_len_us: i64) -> i64 {
    RAMP_US.min((event_len_us / 2).max(1))
}

/// Pure function: `Vec<ZoomTrigger> + ZoomIntensity -> Vec<Keyframe>` (this
/// phase's exact required signature). `clip_id` is stamped onto every
/// produced `Keyframe` (the schema's own foreign-key field). `Off` always
/// returns an empty `Vec` without even looking at `triggers` (module doc
/// comment).
pub fn generate_zoom_keyframes(
    triggers: &[ZoomTrigger],
    intensity: ZoomIntensity,
    clip_id: &str,
) -> Vec<Keyframe> {
    if intensity == ZoomIntensity::Off {
        return Vec::new();
    }
    let peak = intensity.peak_scale();
    let merged = merge_triggers(triggers);

    let mut keyframes = Vec::with_capacity(merged.len() * 4);
    for t in &merged {
        let len = (t.end_us - t.start_us).max(1);
        let ramp = clamp_ramp_us(len);
        let times = [
            t.start_us,
            t.start_us + ramp,
            (t.end_us - ramp).max(t.start_us + ramp),
            t.end_us,
        ];
        let values = [1.0, peak, peak, 1.0];
        for (time_offset_us, value) in times.into_iter().zip(values) {
            keyframes.push(Keyframe {
                id: uuid::Uuid::new_v4().to_string(),
                clip_id: clip_id.to_string(),
                property: "scale".to_string(),
                time_offset_us,
                value,
                curve: "linear".to_string(),
            });
        }
    }
    keyframes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(start_us: i64, end_us: i64) -> Scene {
        Scene {
            id: uuid::Uuid::new_v4().to_string(),
            start_us,
            end_us,
            thumbnail_path: None,
            score: 0.0,
        }
    }

    // -- trigger detectors ---------------------------------------------------

    #[test]
    fn static_scene_triggers_only_keeps_scenes_at_or_above_the_threshold() {
        let scenes = vec![
            scene(0, 1_000_000),                                        // 1s: too short
            scene(1_000_000, 1_000_000 + STATIC_SCENE_MIN_DURATION_US), // exactly the threshold
        ];
        let triggers = static_scene_triggers(&scenes);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].start_us, 1_000_000);
    }

    #[test]
    fn manual_marker_triggers_centers_a_window_on_each_timestamp() {
        let triggers = manual_marker_triggers(&[10_000_000]);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].start_us, 10_000_000 - RAMP_US);
        assert_eq!(triggers[0].end_us, 10_000_000 + RAMP_US);
    }

    #[test]
    fn manual_marker_triggers_clamps_a_marker_near_zero() {
        let triggers = manual_marker_triggers(&[100_000]);
        assert_eq!(triggers[0].start_us, 0); // would otherwise be negative
    }

    #[test]
    fn emphasis_triggers_only_keeps_windows_at_or_above_threshold() {
        let windows = vec![
            EmphasisWindow {
                start_us: 0,
                end_us: 1_000_000,
                energy: 0.2,
            },
            EmphasisWindow {
                start_us: 1_000_000,
                end_us: 2_000_000,
                energy: 0.9,
            },
        ];
        let triggers = emphasis_triggers(&windows);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].start_us, 1_000_000);
    }

    // -- merge_triggers -------------------------------------------------------

    #[test]
    fn merge_triggers_combines_overlapping_events() {
        let triggers = vec![
            ZoomTrigger {
                start_us: 0,
                end_us: 2_000_000,
                reason: "a".into(),
            },
            ZoomTrigger {
                start_us: 1_000_000,
                end_us: 3_000_000,
                reason: "b".into(),
            },
        ];
        let merged = merge_triggers(&triggers);
        assert_eq!(merged.len(), 1);
        assert_eq!((merged[0].start_us, merged[0].end_us), (0, 3_000_000));
    }

    #[test]
    fn merge_triggers_combines_events_within_the_min_gap() {
        let triggers = vec![
            ZoomTrigger {
                start_us: 0,
                end_us: 1_000_000,
                reason: "a".into(),
            },
            ZoomTrigger {
                start_us: 1_000_000 + MIN_GAP_US - 1,
                end_us: 2_000_000,
                reason: "b".into(),
            },
        ];
        let merged = merge_triggers(&triggers);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_triggers_keeps_events_beyond_the_min_gap_separate() {
        let triggers = vec![
            ZoomTrigger {
                start_us: 0,
                end_us: 1_000_000,
                reason: "a".into(),
            },
            ZoomTrigger {
                start_us: 1_000_000 + MIN_GAP_US + 1,
                end_us: 2_000_000,
                reason: "b".into(),
            },
        ];
        let merged = merge_triggers(&triggers);
        assert_eq!(merged.len(), 2);
    }

    // -- generate_zoom_keyframes ----------------------------------------------

    #[test]
    fn off_intensity_produces_no_keyframes_at_all() {
        let triggers = vec![ZoomTrigger {
            start_us: 0,
            end_us: 2_000_000,
            reason: "x".into(),
        }];
        assert!(generate_zoom_keyframes(&triggers, ZoomIntensity::Off, "c1").is_empty());
    }

    #[test]
    fn each_intensity_produces_the_documented_peak_scale() {
        let triggers = vec![ZoomTrigger {
            start_us: 0,
            end_us: 2_000_000,
            reason: "x".into(),
        }];
        for (intensity, expected_peak) in [
            (ZoomIntensity::Low, 1.05),
            (ZoomIntensity::Medium, 1.08),
            (ZoomIntensity::High, 1.15),
        ] {
            let kfs = generate_zoom_keyframes(&triggers, intensity, "c1");
            assert_eq!(kfs.len(), 4);
            let peak = kfs.iter().map(|k| k.value).fold(0.0f64, f64::max);
            assert!((peak - expected_peak).abs() < 1e-9, "{intensity:?}: {peak}");
        }
    }

    #[test]
    fn keyframe_quadruplet_starts_and_ends_at_unity_scale() {
        let triggers = vec![ZoomTrigger {
            start_us: 1_000_000,
            end_us: 3_000_000,
            reason: "x".into(),
        }];
        let kfs = generate_zoom_keyframes(&triggers, ZoomIntensity::Medium, "c1");
        assert_eq!(kfs.len(), 4);
        assert_eq!(kfs[0].time_offset_us, 1_000_000);
        assert_eq!(kfs[0].value, 1.0);
        assert_eq!(kfs[3].time_offset_us, 3_000_000);
        assert_eq!(kfs[3].value, 1.0);
        assert_eq!(kfs[1].value, 1.08);
        assert_eq!(kfs[2].value, 1.08);
        // Strictly increasing time offsets (no crossed/degenerate keyframes).
        assert!(kfs[0].time_offset_us < kfs[1].time_offset_us);
        assert!(kfs[1].time_offset_us <= kfs[2].time_offset_us);
        assert!(kfs[2].time_offset_us < kfs[3].time_offset_us);
    }

    #[test]
    fn every_keyframe_is_stamped_with_the_given_clip_id_and_scale_property() {
        let triggers = vec![ZoomTrigger {
            start_us: 0,
            end_us: 2_000_000,
            reason: "x".into(),
        }];
        let kfs = generate_zoom_keyframes(&triggers, ZoomIntensity::Low, "clip-42");
        assert!(kfs.iter().all(|k| k.clip_id == "clip-42"));
        assert!(kfs.iter().all(|k| k.property == "scale"));
    }

    #[test]
    fn a_very_short_trigger_still_produces_non_crossing_keyframes() {
        // Event shorter than 2*RAMP_US: ramp must shrink rather than cross.
        let triggers = vec![ZoomTrigger {
            start_us: 0,
            end_us: 200_000,
            reason: "x".into(),
        }];
        let kfs = generate_zoom_keyframes(&triggers, ZoomIntensity::Medium, "c1");
        assert_eq!(kfs.len(), 4);
        assert!(kfs[1].time_offset_us <= kfs[2].time_offset_us);
        assert!(kfs[0].time_offset_us < kfs[3].time_offset_us);
    }

    #[test]
    fn overlapping_triggers_produce_one_merged_quadruplet_not_two_stacked_ones() {
        let triggers = vec![
            ZoomTrigger {
                start_us: 0,
                end_us: 2_000_000,
                reason: "a".into(),
            },
            ZoomTrigger {
                start_us: 1_000_000,
                end_us: 3_000_000,
                reason: "b".into(),
            },
        ];
        let kfs = generate_zoom_keyframes(&triggers, ZoomIntensity::Medium, "c1");
        // Merged into one [0, 3_000_000) event -> exactly one quadruplet.
        assert_eq!(kfs.len(), 4);
        assert_eq!(kfs[0].time_offset_us, 0);
        assert_eq!(kfs[3].time_offset_us, 3_000_000);
    }
}
