//! Smoothing/interpolation over raw `SubjectPosition` samples (master
//! prompt §23: "Generate smooth position keyframes... Prevent camera
//! jumping... Use smoothing/interpolation.") — deliberately independent of
//! which `SubjectTracker` implementation produced the raw samples (the
//! whole point of the trait abstraction in `provider`): whether positions
//! come from motion tracking, or one day from a face/person-detection
//! provider, they go through the exact same smoothing here.
//!
//! ## Algorithm: time-based exponential smoothing
//!
//! A plain exponential moving average generalized to *unevenly spaced*
//! samples (real timestamps, not a fixed array index): each new raw sample
//! is blended into the running smoothed value by a factor that depends on
//! how much real time (`dt`) elapsed since the previous sample —
//! `alpha = 1 - exp(-dt / tau)` — so a gap between samples (or a future
//! tracker with irregular sampling) still smooths correctly rather than
//! assuming one fixed sample rate. `tau_us` is the time constant: roughly
//! how long a step change in the raw signal takes to reach ~63% of the way
//! to its new value. Larger `tau_us` means heavier smoothing (less jitter,
//! more lag); smaller means the opposite.
//!
//! This is a real, well-understood, causal (no lookahead) low-pass filter —
//! one of the two techniques this pass's task brief names verbatim ("a
//! moving average or exponential smoothing").
//!
//! ## `project::Keyframe` conversion
//!
//! `keyframes_from_smoothed` turns smoothed positions into real
//! `project::Keyframe` entries, matching two conventions Phase 9 already
//! established (see `capcut::keyframe` module doc comment):
//!
//! - **Time**: `Keyframe::time_offset_us` is absolute project-timeline time
//!   (not source-relative), so a `SubjectPosition::time_us` — which is
//!   source-file-relative, per `provider`'s doc comment — is shifted by the
//!   caller-supplied `clip_position_us` (the clip's own on-timeline start).
//! - **Value units**: `ClipSettings::transform_x/y`'s established
//!   half-canvas-width/height, y-up convention (`project::types` module —
//!   "NOT pixels"), not a second unit system invented for this feature.
//!   `SubjectPosition::target_x/y` are image-space (0.0..=1.0, y-down); see
//!   [`to_half_canvas`] for the conversion.

use uuid::Uuid;

use crate::project::Keyframe;

use super::provider::SubjectPosition;

/// Default smoothing time constant: 0.7 real seconds. Heavy enough that a
/// single noisy sample can't yank the crop window, light enough that a
/// deliberate, sustained subject movement still gets followed within under
/// a second — a reasonable default for "prevent camera jumping" without
/// making the reframe feel unresponsive.
pub const DEFAULT_SMOOTHING_TAU_US: i64 = 700_000;

/// Applies time-based exponential smoothing (module doc comment) to `raw`,
/// which must already be sorted ascending by `time_us` (every
/// `SubjectTracker::track` implementation returns its samples in that
/// order). Returns one smoothed sample per raw sample, same timestamps —
/// this is a filter over the *values*, not a resampling.
pub fn smooth_positions(raw: &[SubjectPosition], tau_us: i64) -> Vec<SubjectPosition> {
    let Some((first, rest)) = raw.split_first() else {
        return Vec::new();
    };

    let tau = (tau_us.max(1)) as f64;
    let mut smoothed_x = first.target_x as f64;
    let mut smoothed_y = first.target_y as f64;
    let mut out = Vec::with_capacity(raw.len());
    out.push(*first);

    let mut prev_time_us = first.time_us;
    for sample in rest {
        let dt = (sample.time_us - prev_time_us).max(0) as f64;
        let alpha = 1.0 - (-dt / tau).exp();
        smoothed_x += alpha * (sample.target_x as f64 - smoothed_x);
        smoothed_y += alpha * (sample.target_y as f64 - smoothed_y);
        out.push(SubjectPosition {
            time_us: sample.time_us,
            target_x: smoothed_x as f32,
            target_y: smoothed_y as f32,
        });
        prev_time_us = sample.time_us;
    }
    out
}

/// Converts a normalized image-space target position (`provider`'s
/// convention: `0.0..=1.0`, origin top-left) into
/// `ClipSettings::transform_x/y`'s half-canvas, y-up convention (module doc
/// comment): `target_x=0.0` (left edge) -> `-1.0`, `target_x=1.0` (right
/// edge) -> `1.0`, `target_x=0.5` (center) -> `0.0`; `target_y` the same but
/// flipped, since half-canvas is y-up while image space is y-down.
fn to_half_canvas(target_x: f32, target_y: f32) -> (f64, f64) {
    let x = (target_x as f64 - 0.5) * 2.0;
    let y = (0.5 - target_y as f64) * 2.0;
    (x, y)
}

/// Turns smoothed positions into real `project::Keyframe` entries — two per
/// sample (`position_x` and `position_y`, the two properties
/// `capcut::keyframe::KeyframeProperty::from_project_property` already
/// recognizes), sharing `clip_id` and `curve: "linear"` (matching
/// `pyJianYingDraft`'s own linear-only-today convention noted on
/// `project::types::Keyframe::curve`'s doc comment). This is real,
/// interpolatable per-time-position data a caller can hand straight to the
/// same rendering/CapCut-export pipeline every other position keyframe goes
/// through — not an abstract type nobody downstream consumes.
pub fn keyframes_from_smoothed(
    smoothed: &[SubjectPosition],
    clip_id: &str,
    clip_position_us: i64,
) -> Vec<Keyframe> {
    let mut out = Vec::with_capacity(smoothed.len() * 2);
    for sample in smoothed {
        let (x, y) = to_half_canvas(sample.target_x, sample.target_y);
        let time_offset_us = clip_position_us + sample.time_us;
        out.push(Keyframe {
            id: Uuid::new_v4().to_string(),
            clip_id: clip_id.to_string(),
            property: "position_x".to_string(),
            time_offset_us,
            value: x,
            curve: "linear".to_string(),
        });
        out.push(Keyframe {
            id: Uuid::new_v4().to_string(),
            clip_id: clip_id.to_string(),
            property: "position_y".to_string(),
            time_offset_us,
            value: y,
            curve: "linear".to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_empty_output() {
        assert!(smooth_positions(&[], DEFAULT_SMOOTHING_TAU_US).is_empty());
    }

    #[test]
    fn a_single_sample_passes_through_unchanged() {
        let raw = vec![SubjectPosition {
            time_us: 1_000,
            target_x: 0.3,
            target_y: 0.7,
        }];
        let smoothed = smooth_positions(&raw, DEFAULT_SMOOTHING_TAU_US);
        assert_eq!(smoothed, raw);
    }

    #[test]
    fn to_half_canvas_maps_corners_and_center() {
        assert_eq!(to_half_canvas(0.0, 0.0), (-1.0, 1.0));
        assert_eq!(to_half_canvas(1.0, 1.0), (1.0, -1.0));
        assert_eq!(to_half_canvas(0.5, 0.5), (0.0, 0.0));
    }

    /// The load-bearing test this pass's brief requires: deliberately noisy
    /// raw input's own frame-to-frame delta must be measurably larger than
    /// the smoothed output's, proving smoothing actually reduces jitter
    /// ("prevent camera jumping") rather than merely producing *some*
    /// numbers.
    #[test]
    fn smoothing_measurably_reduces_frame_to_frame_jitter() {
        // 30 samples, 100ms apart, jittering by ±0.15 around a slow trend
        // from 0.3 to 0.6 — deliberately noisy input.
        let mut raw = Vec::new();
        for i in 0..30 {
            let t = i as f32 / 29.0;
            let trend = 0.3 + 0.3 * t;
            let jitter = if i % 2 == 0 { 0.15 } else { -0.15 };
            raw.push(SubjectPosition {
                time_us: i as i64 * 100_000,
                target_x: trend + jitter,
                target_y: 0.5,
            });
        }

        let smoothed = smooth_positions(&raw, DEFAULT_SMOOTHING_TAU_US);
        assert_eq!(smoothed.len(), raw.len());

        let mean_abs_delta = |samples: &[SubjectPosition]| -> f64 {
            let deltas: Vec<f64> = samples
                .windows(2)
                .map(|w| (w[1].target_x - w[0].target_x).abs() as f64)
                .collect();
            deltas.iter().sum::<f64>() / deltas.len() as f64
        };

        let raw_jitter = mean_abs_delta(&raw);
        let smoothed_jitter = mean_abs_delta(&smoothed);

        assert!(
            smoothed_jitter < raw_jitter * 0.5,
            "expected smoothing to at least halve mean frame-to-frame delta: raw={raw_jitter} smoothed={smoothed_jitter}"
        );
    }

    #[test]
    fn a_larger_time_constant_smooths_more_than_a_smaller_one() {
        let raw: Vec<SubjectPosition> = (0..10)
            .map(|i| SubjectPosition {
                time_us: i as i64 * 200_000,
                target_x: if i % 2 == 0 { 0.2 } else { 0.8 },
                target_y: 0.5,
            })
            .collect();

        let lightly_smoothed = smooth_positions(&raw, 50_000);
        let heavily_smoothed = smooth_positions(&raw, 5_000_000);

        let mean_abs_delta = |samples: &[SubjectPosition]| -> f64 {
            let deltas: Vec<f64> = samples
                .windows(2)
                .map(|w| (w[1].target_x - w[0].target_x).abs() as f64)
                .collect();
            deltas.iter().sum::<f64>() / deltas.len() as f64
        };

        assert!(mean_abs_delta(&heavily_smoothed) < mean_abs_delta(&lightly_smoothed));
    }

    #[test]
    fn keyframes_from_smoothed_produces_position_x_and_y_pairs_with_absolute_time() {
        let smoothed = vec![
            SubjectPosition {
                time_us: 0,
                target_x: 0.5,
                target_y: 0.5,
            },
            SubjectPosition {
                time_us: 500_000,
                target_x: 0.75,
                target_y: 0.25,
            },
        ];
        let clip_position_us = 10_000_000;
        let keyframes = keyframes_from_smoothed(&smoothed, "clip-1", clip_position_us);

        assert_eq!(keyframes.len(), 4);
        for kf in &keyframes {
            assert_eq!(kf.clip_id, "clip-1");
            assert_eq!(kf.curve, "linear");
        }

        let x_keyframes: Vec<&Keyframe> = keyframes
            .iter()
            .filter(|k| k.property == "position_x")
            .collect();
        let y_keyframes: Vec<&Keyframe> = keyframes
            .iter()
            .filter(|k| k.property == "position_y")
            .collect();
        assert_eq!(x_keyframes.len(), 2);
        assert_eq!(y_keyframes.len(), 2);

        assert_eq!(x_keyframes[0].time_offset_us, clip_position_us);
        assert_eq!(x_keyframes[0].value, 0.0); // 0.5 -> center -> 0.0
        assert_eq!(x_keyframes[1].time_offset_us, clip_position_us + 500_000);
        assert_eq!(x_keyframes[1].value, 0.5); // 0.75 -> half-canvas 0.5

        assert_eq!(y_keyframes[1].value, 0.5); // target_y=0.25 -> (0.5-0.25)*2 = 0.5
    }
}
