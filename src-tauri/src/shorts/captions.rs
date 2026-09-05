//! Transcript slicing/re-timing for one generated short's own span
//! (master prompt §22's "Captions" pipeline stage feeding off the
//! "Clip Extraction" stage's span).
//!
//! A generated short's clip places `[span_start_us, span_end_us)` of the
//! *source* media at `position_us: 0` on its own brand-new, single-clip
//! project timeline (`shorts::build::build_short_project`). Any transcript
//! entry (and its per-word timings) that overlaps that span must therefore
//! be re-expressed relative to the short's own timeline — a caption that was
//! at absolute source time `T` lands at `T - span_start_us` in the new
//! project, not still at `T` (this module's whole reason to exist; the
//! caption-retiming bug this pipeline's test suite specifically checks for).

use crate::project::{TranscriptEntry, Word};

/// Returns every `entries` item that overlaps `[span_start_us, span_end_us)`,
/// clipped to that span and re-timed to be relative to `span_start_us`
/// (i.e. the first microsecond of the span becomes timestamp `0`). Entries
/// entirely outside the span are dropped; an entry that only partially
/// overlaps is trimmed to the overlapping portion (never left extending
/// past the new project's own `[0, span_end_us - span_start_us)` bounds).
/// Per-word timings are trimmed/re-timed the same way, and a word entirely
/// outside the span is dropped from its entry's `words` list.
pub fn slice_transcript_for_span(
    entries: &[TranscriptEntry],
    span_start_us: i64,
    span_end_us: i64,
) -> Vec<TranscriptEntry> {
    if span_end_us <= span_start_us {
        return Vec::new();
    }

    entries
        .iter()
        .filter(|e| e.end_us > span_start_us && e.start_us < span_end_us)
        .map(|e| {
            let start_us = clip_and_retime(e.start_us, span_start_us, span_end_us);
            let end_us = clip_and_retime(e.end_us, span_start_us, span_end_us);
            let words = e
                .words
                .iter()
                .filter(|w| w.end_us > span_start_us && w.start_us < span_end_us)
                .map(|w| Word {
                    text: w.text.clone(),
                    start_us: clip_and_retime(w.start_us, span_start_us, span_end_us),
                    end_us: clip_and_retime(w.end_us, span_start_us, span_end_us),
                    confidence: w.confidence,
                })
                .collect();
            TranscriptEntry {
                id: e.id.clone(),
                media_id: e.media_id.clone(),
                text: e.text.clone(),
                start_us,
                end_us,
                confidence: e.confidence,
                words,
                is_filler: e.is_filler,
            }
        })
        .collect()
}

/// Clamps an absolute source-relative timestamp into `[span_start_us,
/// span_end_us]`, then shifts it to be relative to `span_start_us`.
fn clip_and_retime(absolute_us: i64, span_start_us: i64, span_end_us: i64) -> i64 {
    (absolute_us.clamp(span_start_us, span_end_us) - span_start_us).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start_us: i64, end_us: i64) -> Word {
        Word {
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
        }
    }

    fn entry(
        id: &str,
        text: &str,
        start_us: i64,
        end_us: i64,
        words: Vec<Word>,
    ) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words,
            is_filler: false,
        }
    }

    #[test]
    fn an_entry_fully_inside_the_span_is_retimed_relative_to_span_start() {
        // Entry at absolute [10s, 11s), span is [8s, 20s) -> new project
        // time [2s, 3s).
        let entries = vec![entry(
            "e1",
            "hello",
            10_000_000,
            11_000_000,
            vec![word("hello", 10_000_000, 11_000_000)],
        )];
        let sliced = slice_transcript_for_span(&entries, 8_000_000, 20_000_000);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced[0].start_us, 2_000_000);
        assert_eq!(sliced[0].end_us, 3_000_000);
        assert_eq!(sliced[0].words[0].start_us, 2_000_000);
        assert_eq!(sliced[0].words[0].end_us, 3_000_000);
    }

    #[test]
    fn an_entry_entirely_outside_the_span_is_dropped() {
        let entries = vec![entry("e1", "later", 30_000_000, 31_000_000, vec![])];
        let sliced = slice_transcript_for_span(&entries, 0, 10_000_000);
        assert!(sliced.is_empty());
    }

    #[test]
    fn an_entry_partially_overlapping_the_span_start_is_clipped() {
        // Entry [4s, 6s), span [5s, 15s) -> clipped to [5s,6s) then retimed
        // to [0s, 1s).
        let entries = vec![entry("e1", "partial", 4_000_000, 6_000_000, vec![])];
        let sliced = slice_transcript_for_span(&entries, 5_000_000, 15_000_000);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced[0].start_us, 0);
        assert_eq!(sliced[0].end_us, 1_000_000);
    }

    #[test]
    fn an_entry_partially_overlapping_the_span_end_is_clipped() {
        // Entry [9s, 12s), span [0s, 10s) -> clipped to [9s,10s) then
        // retimed to [9s, 10s) (span starts at 0, no shift).
        let entries = vec![entry("e1", "partial", 9_000_000, 12_000_000, vec![])];
        let sliced = slice_transcript_for_span(&entries, 0, 10_000_000);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced[0].start_us, 9_000_000);
        assert_eq!(sliced[0].end_us, 10_000_000);
    }

    #[test]
    fn words_partially_outside_the_span_are_dropped_from_the_entry() {
        let entries = vec![entry(
            "e1",
            "one two three",
            0,
            3_000_000,
            vec![
                word("one", 0, 1_000_000),
                word("two", 1_000_000, 2_000_000),
                word("three", 2_000_000, 3_000_000),
            ],
        )];
        // Span only covers [0, 2s) -> "three" (starts at 2s, entirely at/after
        // the boundary) is dropped, "two" is clipped.
        let sliced = slice_transcript_for_span(&entries, 0, 2_000_000);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced[0].words.len(), 2);
        assert_eq!(sliced[0].words[1].text, "two");
        assert_eq!(sliced[0].words[1].end_us, 2_000_000);
    }

    #[test]
    fn an_invalid_or_empty_span_produces_no_captions() {
        let entries = vec![entry("e1", "x", 0, 1_000_000, vec![])];
        assert!(slice_transcript_for_span(&entries, 5_000_000, 5_000_000).is_empty());
        assert!(slice_transcript_for_span(&entries, 5_000_000, 1_000_000).is_empty());
    }

    #[test]
    fn empty_entries_produces_no_captions() {
        assert!(slice_transcript_for_span(&[], 0, 10_000_000).is_empty());
    }
}
