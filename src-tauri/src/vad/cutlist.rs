//! Cut-list generation: speech segments → padded/merged "keep" regions →
//! gaps become proposed `Cut { kind: Remove, reason: Silence, applied:
//! false }` entries, feeding the Phase 4 timeline engine (via
//! `timeline::silence::apply_cuts_to_clip`) rather than autocut's own
//! standalone single-`CutList` model (`docs/architecture-audit.md` §8).
//!
//! Reimplemented (not copied) from `vendor/autocut/src-tauri/src/cutlist.rs`'s
//! `CutList::from_speech_segments` padding/merge/gap-inversion logic, with
//! two changes beyond the f64-seconds → i64-microsecond rewrite:
//!
//! 1. Padding is two independent values (`padding_before_us`/
//!    `padding_after_us`), not autocut's single symmetric `pad` — master
//!    prompt §12 lists "padding before" and "padding after" separately.
//! 2. A second `merge_gap_us` knob ("merge nearby speech", also listed
//!    independently in §12): a coarser pass over *already-padded* keep
//!    regions, distinct from VAD's own `min_silence_us` (which operates on
//!    raw per-chunk probabilities before padding even exists). In practice
//!    both padding-overlap merging and gap-based merging reduce to the same
//!    "merge if the gap between two keep regions is `<= threshold`" rule
//!    (an overlap is just a negative gap), so both happen in one merge pass
//!    with `threshold = merge_gap_us.max(0)` — mathematically equivalent to
//!    running the overlap-merge and gap-merge passes separately, since
//!    interval-merging by sorted start is associative.
//!
//! Output is *only* the `Remove` intervals (the silence gaps) — the "keep"
//! regions are implicit (whatever isn't covered by a `Remove` cut), which is
//! all `timeline::silence::apply_cuts_to_clip` needs to translate this into
//! split+delete operations on the real timeline.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{Cut, CutKind, CutReason};

use super::provider::SpeechSegment;

/// Everything master prompt §12 lists that isn't already a `VadParams`
/// concern (threshold/min-silence/min-speech live in `super::provider`,
/// applied *before* this stage ever sees the segments).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
pub struct CutParams {
    pub padding_before_us: i64,
    pub padding_after_us: i64,
    /// "Merge nearby speech": two kept (already-padded) regions closer than
    /// this are merged into one, purely for a cleaner preview — this never
    /// re-runs VAD segmentation.
    pub merge_gap_us: i64,
}

/// Builds the proposed (`applied: false`) `Remove` cuts for `source_media_id`
/// from `segments`, per `CutParams`. `media_duration_us` bounds the padding
/// clamp and supplies the tail/whole-file `Remove` when speech doesn't reach
/// the end (or exists at all).
pub fn build_cuts_from_speech_segments(
    segments: &[SpeechSegment],
    source_media_id: &str,
    media_duration_us: i64,
    params: CutParams,
) -> Vec<Cut> {
    let mut kept: Vec<(i64, i64)> = segments
        .iter()
        .map(|s| {
            (
                (s.start_us - params.padding_before_us).max(0),
                (s.end_us + params.padding_after_us).min(media_duration_us),
            )
        })
        .filter(|(s, e)| e > s)
        .collect();
    kept.sort_by_key(|&(s, _)| s);

    let merge_gap = params.merge_gap_us.max(0);
    let mut merged: Vec<(i64, i64)> = Vec::with_capacity(kept.len());
    for (s, e) in kept {
        if let Some(last) = merged.last_mut() {
            if s - last.1 <= merge_gap {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }

    let mut cuts = Vec::with_capacity(merged.len() + 1);
    let mut cursor = 0i64;
    for &(s, e) in &merged {
        if s > cursor {
            cuts.push(new_remove_cut(source_media_id, cursor, s));
        }
        cursor = e.max(cursor);
    }
    if cursor < media_duration_us {
        cuts.push(new_remove_cut(source_media_id, cursor, media_duration_us));
    }
    cuts
}

fn new_remove_cut(source_media_id: &str, start_us: i64, end_us: i64) -> Cut {
    Cut {
        id: uuid::Uuid::new_v4().to_string(),
        kind: CutKind::Remove,
        source_media_id: source_media_id.to_string(),
        start_us,
        end_us,
        reason: CutReason::Silence,
        applied: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_us: i64, end_us: i64) -> SpeechSegment {
        SpeechSegment {
            start_us,
            end_us,
            confidence: 0.9,
        }
    }

    fn removes(cuts: &[Cut]) -> Vec<(i64, i64)> {
        cuts.iter()
            .map(|c| {
                assert_eq!(c.kind, CutKind::Remove);
                assert_eq!(c.reason, CutReason::Silence);
                assert!(!c.applied);
                (c.start_us, c.end_us)
            })
            .collect()
    }

    #[test]
    fn single_segment_padded_produces_remove_before_and_after() {
        let segs = [seg(1_000_000, 2_000_000)];
        let params = CutParams {
            padding_before_us: 300_000,
            padding_after_us: 300_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params);
        // Kept region: [0.7s, 2.3s]. Removes: [0, 0.7) and [2.3, 5.0).
        assert_eq!(removes(&cuts), vec![(0, 700_000), (2_300_000, 5_000_000)]);
        assert!(cuts.iter().all(|c| c.source_media_id == "m1"));
    }

    #[test]
    fn independent_before_and_after_padding_are_respected() {
        let segs = [seg(1_000_000, 2_000_000)];
        let params = CutParams {
            padding_before_us: 100_000,
            padding_after_us: 500_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params);
        assert_eq!(removes(&cuts), vec![(0, 900_000), (2_500_000, 5_000_000)]);
    }

    #[test]
    fn overlapping_padded_regions_merge() {
        let segs = [seg(1_000_000, 2_000_000), seg(2_400_000, 3_000_000)];
        let params = CutParams {
            padding_before_us: 300_000,
            padding_after_us: 300_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params);
        // Padded: [0.7,2.3] and [2.1,3.3] overlap -> merges to [0.7,3.3].
        assert_eq!(removes(&cuts), vec![(0, 700_000), (3_300_000, 5_000_000)]);
    }

    #[test]
    fn merge_gap_merges_two_kept_regions_that_do_not_overlap() {
        let segs = [seg(1_000_000, 2_000_000), seg(2_500_000, 3_000_000)];
        // No padding: kept regions [1.0,2.0] and [2.5,3.0], gap 0.5s.
        let params_no_merge = CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params_no_merge);
        assert_eq!(
            removes(&cuts),
            vec![
                (0, 1_000_000),
                (2_000_000, 2_500_000),
                (3_000_000, 5_000_000)
            ]
        );

        let params_merged = CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 600_000, // wider than the 0.5s gap -> merges
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params_merged);
        assert_eq!(removes(&cuts), vec![(0, 1_000_000), (3_000_000, 5_000_000)]);
    }

    #[test]
    fn empty_segments_produce_a_single_remove_covering_the_whole_media() {
        let cuts = build_cuts_from_speech_segments(&[], "m1", 4_000_000, CutParams::default());
        assert_eq!(removes(&cuts), vec![(0, 4_000_000)]);
    }

    #[test]
    fn empty_segments_with_zero_duration_media_produce_no_cuts() {
        let cuts = build_cuts_from_speech_segments(&[], "m1", 0, CutParams::default());
        assert!(cuts.is_empty());
    }

    #[test]
    fn padding_clamps_at_zero_and_media_duration() {
        let segs = [seg(100_000, 4_900_000)];
        let params = CutParams {
            padding_before_us: 300_000,
            padding_after_us: 300_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, params);
        // Padded region clamps to [0, 5s], covering the whole file -> no removes.
        assert!(cuts.is_empty(), "{cuts:?}");
    }

    #[test]
    fn speech_covering_the_whole_file_produces_no_cuts() {
        let segs = [seg(0, 5_000_000)];
        let cuts = build_cuts_from_speech_segments(&segs, "m1", 5_000_000, CutParams::default());
        assert!(cuts.is_empty());
    }
}
