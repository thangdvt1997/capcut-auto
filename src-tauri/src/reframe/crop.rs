//! Crop-window computation — the actual "auto reframe" output (master
//! prompt §23: 1920x1080 -> 1080x1920, "do NOT simply center crop").
//!
//! Given a smoothed target position (normalized, image-space — see
//! `provider::SubjectPosition`) and a target aspect ratio, computes the
//! largest crop rectangle of that aspect ratio that fits inside the source
//! frame, centered on the target position and clamped so it never extends
//! past the source frame's bounds. Doing this over every smoothed sample
//! yields a real crop-region-over-time result — pixel `x`/`y`/`width`/
//! `height` per timestamp — that a real FFmpeg `crop` filter (or a
//! keyframed sequence of one) can consume directly.
//!
//! **Composition note**: `render::plan` (as of this pass) has no existing
//! *time-varying* per-clip filter-parameter convention to plug into yet —
//! every `ClipSettings`/keyframe value it emits today is evaluated once per
//! clip, not as a function of `t` (its own module doc comment: `Caption`/
//! `Effect` nodes are the only "not wired up yet" gap it documents, and
//! neither of those is a time-varying-parameter mechanism either). Auto-zoom
//! (`IMPLEMENTATION_PLAN.md` Phase 11, a concurrently developed feature) is
//! the first place such a convention would likely land. Rather than invent
//! a competing time-varying-filter-string convention here, this module
//! produces the crop-region-over-time result as a plain, real, directly
//! testable data sequence (`Vec<CropWindow>`) — master prompt §23's own
//! "or a sequence of keyframed crop parameters" alternative — plus a
//! single-window static `crop=` filter string
//! ([`crop_window_ffmpeg_filter`]) for immediate, no-plumbing-required use.
//! When `render::plan` grows a real time-varying-parameter mechanism, this
//! is the one place a matching time-varying `crop` expression should be
//! added, composed alongside zoom rather than duplicating it.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::provider::SubjectPosition;

/// One crop rectangle at a point in time, in source-frame pixel units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CropWindow {
    pub time_us: i64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The largest `target_width`x`target_height`-aspect rectangle that fits
/// inside a `source_width`x`source_height` frame, centered on the
/// normalized `(target_x, target_y)` position and clamped so it never
/// extends past the source frame's bounds (a target position near an edge
/// still produces a fully in-bounds crop window, not one that overhangs).
///
/// Aspect math: whichever source dimension is the *tighter* constraint for
/// the target aspect ratio determines the crop size — e.g. converting a
/// 1920x1080 (16:9) source to a 9:16 target: the target aspect is narrower
/// than the source, so the crop keeps the full source height (1080) and
/// derives a matching width (1080 * 9/16 = 607.5 -> 608).
pub fn compute_crop_window(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    target_x: f32,
    target_y: f32,
) -> CropWindow {
    let source_aspect = source_width as f64 / source_height as f64;
    let target_aspect = target_width as f64 / target_height as f64;

    let (crop_width, crop_height) = if target_aspect < source_aspect {
        // Target is relatively narrower than source -> keep full height,
        // derive width.
        let height = source_height;
        let width = ((height as f64 * target_aspect).round() as u32)
            .min(source_width)
            .max(1);
        (width, height)
    } else {
        // Target is relatively wider than (or equal to) source -> keep
        // full width, derive height.
        let width = source_width;
        let height = ((width as f64 / target_aspect).round() as u32)
            .min(source_height)
            .max(1);
        (width, height)
    };

    let center_x = target_x as f64 * source_width as f64;
    let center_y = target_y as f64 * source_height as f64;

    let max_x = source_width.saturating_sub(crop_width);
    let max_y = source_height.saturating_sub(crop_height);

    let x = (center_x - crop_width as f64 / 2.0)
        .round()
        .clamp(0.0, max_x as f64) as u32;
    let y = (center_y - crop_height as f64 / 2.0)
        .round()
        .clamp(0.0, max_y as f64) as u32;

    CropWindow {
        time_us: 0,
        x,
        y,
        width: crop_width,
        height: crop_height,
    }
}

/// Applies [`compute_crop_window`] to every sample in `positions`, carrying
/// each sample's own `time_us` through — the real crop-region-over-time
/// result (module doc comment).
pub fn crop_windows_over_time(
    positions: &[SubjectPosition],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Vec<CropWindow> {
    positions
        .iter()
        .map(|p| {
            let mut window = compute_crop_window(
                source_width,
                source_height,
                target_width,
                target_height,
                p.target_x,
                p.target_y,
            );
            window.time_us = p.time_us;
            window
        })
        .collect()
}

/// Renders one `CropWindow` as ffmpeg's own `crop=w:h:x:y` filter syntax —
/// usable standalone for a single (e.g. first-sample) static crop, per this
/// module's doc comment.
pub fn crop_window_ffmpeg_filter(window: &CropWindow) -> String {
    format!(
        "crop={}:{}:{}:{}",
        window.width, window.height, window.x, window.y
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_target_from_landscape_source_keeps_full_height() {
        // The master prompt's own worked example: 1920x1080 -> 9:16.
        let window = compute_crop_window(1920, 1080, 9, 16, 0.5, 0.5);
        assert_eq!(window.height, 1080);
        assert_eq!(window.width, 608); // round(1080 * 9/16) = round(607.5) = 608
                                       // Centered target -> centered crop.
        assert_eq!(window.x, (1920 - 608) / 2);
        assert_eq!(window.y, 0);
    }

    #[test]
    fn landscape_target_from_portrait_source_keeps_full_width() {
        let window = compute_crop_window(1080, 1920, 16, 9, 0.5, 0.5);
        assert_eq!(window.width, 1080);
        assert_eq!(window.height, 608); // round(1080 * 9/16)
        assert_eq!(window.x, 0);
        assert_eq!(window.y, (1920 - 608) / 2);
    }

    #[test]
    fn square_target_from_landscape_source() {
        let window = compute_crop_window(1920, 1080, 1, 1, 0.5, 0.5);
        assert_eq!(window.width, 1080);
        assert_eq!(window.height, 1080);
    }

    #[test]
    fn a_target_near_the_left_edge_still_produces_an_in_bounds_window() {
        // target_x near 0.0 - a naive center-on-target computation would
        // put the crop's left edge at a negative x.
        let window = compute_crop_window(1920, 1080, 9, 16, 0.0, 0.5);
        assert_eq!(window.x, 0);
        assert!(window.x + window.width <= 1920);
    }

    #[test]
    fn a_target_near_the_right_edge_still_produces_an_in_bounds_window() {
        let window = compute_crop_window(1920, 1080, 9, 16, 1.0, 0.5);
        assert_eq!(window.x + window.width, 1920);
        assert!(window.x + window.width <= 1920);
    }

    #[test]
    fn a_target_near_the_top_edge_still_produces_an_in_bounds_window() {
        let window = compute_crop_window(1080, 1920, 16, 9, 0.5, 0.0);
        assert_eq!(window.y, 0);
        assert!(window.y + window.height <= 1920);
    }

    #[test]
    fn a_target_near_the_bottom_edge_still_produces_an_in_bounds_window() {
        let window = compute_crop_window(1080, 1920, 16, 9, 0.5, 1.0);
        assert_eq!(window.y + window.height, 1920);
        assert!(window.y + window.height <= 1920);
    }

    #[test]
    fn crop_windows_over_time_carries_each_samples_own_timestamp() {
        let positions = vec![
            SubjectPosition {
                time_us: 0,
                target_x: 0.2,
                target_y: 0.5,
            },
            SubjectPosition {
                time_us: 500_000,
                target_x: 0.8,
                target_y: 0.5,
            },
        ];
        let windows = crop_windows_over_time(&positions, 1920, 1080, 9, 16);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].time_us, 0);
        assert_eq!(windows[1].time_us, 500_000);
        // The crop should have visibly moved rightward following the
        // target, not stayed centered.
        assert!(windows[1].x > windows[0].x);
    }

    #[test]
    fn ffmpeg_filter_string_matches_crop_filter_syntax() {
        let window = CropWindow {
            time_us: 0,
            x: 10,
            y: 20,
            width: 608,
            height: 1080,
        };
        assert_eq!(crop_window_ffmpeg_filter(&window), "crop=608:1080:10:20");
    }

    #[test]
    fn every_crop_window_stays_within_source_bounds_across_a_swept_target() {
        // Sweep target_x/target_y across the full 0..1 range - every single
        // resulting window must stay fully inside the source frame.
        let steps = 21;
        for i in 0..=steps {
            for j in 0..=steps {
                let tx = i as f32 / steps as f32;
                let ty = j as f32 / steps as f32;
                let window = compute_crop_window(1920, 1080, 9, 16, tx, ty);
                assert!(
                    window.x + window.width <= 1920,
                    "tx={tx} ty={ty} window={window:?}"
                );
                assert!(
                    window.y + window.height <= 1080,
                    "tx={tx} ty={ty} window={window:?}"
                );
            }
        }
    }
}
