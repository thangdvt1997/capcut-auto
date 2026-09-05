//! Candidate ranking (master prompt §22's "Candidate Ranking" pipeline
//! stage): turning a raw `Vec<Highlight>` (already carrying a real `score`,
//! `highlights::types::Highlight`) into the top `clip_count` **non-overlapping**
//! spans, each adjusted to roughly match the requested target duration.
//!
//! ## Why not naive top-N by score
//!
//! Two highlights can legitimately overlap in time (e.g. a broad "the whole
//! interview" candidate and a narrower "one great answer" candidate inside
//! it) — naively sorting by score and taking the first `clip_count` can
//! therefore select two spans that share footage, which is never a sane set
//! of *distinct* shorts to hand back to a user. [`select_top_non_overlapping`]
//! is a real fix for this: a greedy interval-selection pass, described below.
//!
//! ## Algorithm: greedy-by-score, skip conflicts
//!
//! Sort all candidates by `score` descending; walk the sorted list and keep
//! a candidate only if its `[start_us, end_us)` span doesn't overlap any
//! already-accepted span; stop once `clip_count` have been accepted (or the
//! candidate list is exhausted).
//!
//! This is a *greedy* heuristic, not the classic dynamic-programming
//! "weighted interval scheduling" optimal solution (which maximizes the
//! *sum* of scores over all chosen intervals, and can trade one high-score
//! interval for two or three lower-score ones that don't conflict with it).
//! That DP optimizes a different objective than what this feature actually
//! wants: master prompt §22 asks for the *individually best* `clip_count`
//! highlights ("Highlight #1, Score 92" per master prompt §21's own mockup),
//! not the highest-total-score combination — a user picking "5 clips" wants
//! their 5 best individual moments, not whatever combination of moments sums
//! to the largest number. Greedy-by-score is the correct fit for that
//! framing, and it is still fully correct at avoiding overlaps (the actual
//! bug this module's brief calls out), which a naive top-N-by-score pass is
//! not.
//!
//! Returned candidates are re-sorted into chronological (`start_us`)
//! order — a "Highlight #1" that appears earlier in the source video should
//! read as earlier in the result list, independent of which score ranking
//! order they were selected in.

use crate::highlights::types::Highlight;

/// True when `[a_start, a_end)` and `[b_start, b_end)` share any time.
/// Touching-but-not-overlapping spans (`a_end == b_start`) do **not**
/// count as overlapping.
fn spans_overlap(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> bool {
    a_start < b_end && b_start < a_end
}

/// Selects up to `count` non-overlapping highlights, greedily by `score`
/// descending (module doc comment). `count == 0` returns an empty `Vec`
/// without inspecting `highlights` at all. The result is sorted by
/// `start_us` ascending (chronological order), not score order.
pub fn select_top_non_overlapping(highlights: &[Highlight], count: usize) -> Vec<Highlight> {
    if count == 0 {
        return Vec::new();
    }

    let mut by_score: Vec<&Highlight> = highlights.iter().collect();
    by_score.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut selected: Vec<Highlight> = Vec::with_capacity(count.min(highlights.len()));
    for candidate in by_score {
        if selected.len() >= count {
            break;
        }
        let conflicts = selected
            .iter()
            .any(|s| spans_overlap(s.start_us, s.end_us, candidate.start_us, candidate.end_us));
        if !conflicts {
            selected.push(candidate.clone());
        }
    }

    selected.sort_by_key(|h| h.start_us);
    selected
}

/// Expands or contracts `[start_us, end_us)` around its own center to match
/// `target_duration_us` as closely as possible, clamped so the result always
/// stays within `[0, media_duration_us]`.
///
/// ## Exact approach (documented per this pass's brief)
///
/// 1. Compute the span's center: `(start_us + end_us) / 2`.
/// 2. Build a new `target_duration_us`-long window centered on that point:
///    `[center - target/2, center + target/2)`.
/// 3. If that window would start before `0` or end after `media_duration_us`,
///    **shift the whole window** back into bounds (never asymmetrically
///    truncate just the overflowing edge) — a window that overflows the
///    left edge slides right until its start is `0`; one that overflows the
///    right edge slides left until its end is `media_duration_us`. This
///    keeps the window's length exactly `target_duration_us` whenever the
///    media itself is at least that long, rather than silently producing a
///    shorter-than-requested short.
/// 4. If `target_duration_us` itself exceeds `media_duration_us` (a custom
///    duration longer than the source file, or a source shorter than even
///    the smallest fixed preset), the result is clamped to the media's own
///    full `[0, media_duration_us]` span — the largest span that could ever
///    exist, rather than an out-of-bounds or negative-length result.
pub fn adjust_span_to_duration(
    start_us: i64,
    end_us: i64,
    target_duration_us: i64,
    media_duration_us: i64,
) -> (i64, i64) {
    let media_duration_us = media_duration_us.max(0);
    if media_duration_us == 0 {
        return (0, 0);
    }
    let target = target_duration_us.clamp(1, media_duration_us);

    let center = (start_us + end_us) / 2;
    let mut new_start = center - target / 2;
    let mut new_end = new_start + target;

    if new_start < 0 {
        let shift = -new_start;
        new_start += shift;
        new_end += shift;
    }
    if new_end > media_duration_us {
        let shift = new_end - media_duration_us;
        new_end -= shift;
        new_start -= shift;
    }
    // A media file shorter than `target` (already clamped above via
    // `target.clamp(1, media_duration_us)`) can't overflow both edges at
    // once, but guard against float/int edge cases defensively.
    new_start = new_start.max(0);

    (new_start, new_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight(id: &str, start_us: i64, end_us: i64, score: f32) -> Highlight {
        Highlight {
            id: id.to_string(),
            start_us,
            end_us,
            score,
            title: format!("Highlight {id}"),
            reason: "test".to_string(),
        }
    }

    #[test]
    fn selects_the_top_n_by_score_when_none_overlap() {
        let highlights = vec![
            highlight("a", 0, 1_000_000, 50.0),
            highlight("b", 2_000_000, 3_000_000, 90.0),
            highlight("c", 4_000_000, 5_000_000, 70.0),
        ];
        let selected = select_top_non_overlapping(&highlights, 2);
        assert_eq!(selected.len(), 2);
        // Chronological order in the result.
        assert_eq!(selected[0].id, "b");
        assert_eq!(selected[1].id, "c");
    }

    #[test]
    fn a_naive_top_n_would_pick_two_overlapping_spans_this_does_not() {
        // "b" is the single highest score and overlaps both "a" and "c".
        // A naive top-2-by-score pass would pick "b" and "c" (scores 95/70),
        // which genuinely overlap ([1s,4s) vs [3s,6s)). The correct
        // non-overlapping selection must reject "c" once "b" is taken and
        // fall through to the next non-conflicting candidate, "a".
        let highlights = vec![
            highlight("a", 6_000_000, 7_000_000, 60.0),
            highlight("b", 1_000_000, 4_000_000, 95.0),
            highlight("c", 3_000_000, 6_000_000, 70.0),
        ];
        let selected = select_top_non_overlapping(&highlights, 2);
        assert_eq!(selected.len(), 2);
        let ids: Vec<&str> = selected.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"a"));
        assert!(
            !ids.contains(&"c"),
            "expected the overlapping lower-scored 'c' to be rejected: {ids:?}"
        );
        // No two selected spans overlap.
        for i in 0..selected.len() {
            for j in (i + 1)..selected.len() {
                assert!(!spans_overlap(
                    selected[i].start_us,
                    selected[i].end_us,
                    selected[j].start_us,
                    selected[j].end_us
                ));
            }
        }
    }

    #[test]
    fn touching_spans_are_not_considered_overlapping() {
        let highlights = vec![
            highlight("a", 0, 1_000_000, 90.0),
            highlight("b", 1_000_000, 2_000_000, 80.0),
        ];
        let selected = select_top_non_overlapping(&highlights, 2);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn count_zero_returns_nothing() {
        let highlights = vec![highlight("a", 0, 1_000_000, 90.0)];
        assert!(select_top_non_overlapping(&highlights, 0).is_empty());
    }

    #[test]
    fn requesting_more_than_available_returns_all_non_overlapping_candidates() {
        let highlights = vec![highlight("a", 0, 1_000_000, 90.0)];
        let selected = select_top_non_overlapping(&highlights, 5);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn empty_input_returns_nothing() {
        assert!(select_top_non_overlapping(&[], 3).is_empty());
    }

    // -- adjust_span_to_duration --------------------------------------------

    #[test]
    fn expands_a_short_span_around_its_center() {
        let (start, end) = adjust_span_to_duration(4_000_000, 5_000_000, 4_000_000, 100_000_000);
        // Original center is 4.5s; a 4s window centered there is [2.5s, 6.5s).
        assert_eq!(start, 2_500_000);
        assert_eq!(end, 6_500_000);
        assert_eq!(end - start, 4_000_000);
    }

    #[test]
    fn contracts_a_long_span_around_its_center() {
        let (start, end) = adjust_span_to_duration(0, 10_000_000, 4_000_000, 100_000_000);
        // Center is 5s; a 4s window centered there is [3s, 7s).
        assert_eq!(start, 3_000_000);
        assert_eq!(end, 7_000_000);
    }

    #[test]
    fn clamps_by_shifting_when_the_window_would_start_before_zero() {
        let (start, end) = adjust_span_to_duration(0, 500_000, 4_000_000, 100_000_000);
        // Naive center-based window would start negative; it must shift
        // right to start at 0 while keeping the full requested length.
        assert_eq!(start, 0);
        assert_eq!(end, 4_000_000);
    }

    #[test]
    fn clamps_by_shifting_when_the_window_would_end_past_media_duration() {
        let (start, end) = adjust_span_to_duration(9_500_000, 10_000_000, 4_000_000, 10_000_000);
        assert_eq!(end, 10_000_000);
        assert_eq!(start, 6_000_000);
        assert_eq!(end - start, 4_000_000);
    }

    #[test]
    fn a_target_longer_than_the_whole_media_clamps_to_the_full_media_span() {
        let (start, end) = adjust_span_to_duration(1_000_000, 2_000_000, 60_000_000, 5_000_000);
        assert_eq!((start, end), (0, 5_000_000));
    }

    #[test]
    fn a_zero_length_media_produces_a_zero_length_span_without_panicking() {
        assert_eq!(adjust_span_to_duration(0, 0, 4_000_000, 0), (0, 0));
    }
}
