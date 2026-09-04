//! `generate_captions_from_transcript` — turns transcribed speech into
//! `Caption`s with correct line-wrapping (master prompt §26). Pure and
//! stateless: no project, no I/O, fully unit-testable against hand-built
//! `TranscriptEntry` fixtures.
//!
//! Two grouping strategies (`CaptionGenerationSettings::grouping`):
//! - [`CaptionGroupingMode::Sentence`][]: one (or more, if it needs wrapping)
//!   caption(s) per `TranscriptEntry`, never merging words across entries —
//!   captions track the transcript's own sentence/segment boundaries.
//! - [`CaptionGroupingMode::Word`][]: a single continuous word stream flows
//!   across *every* entry, chunked purely by `max_words_per_line`/
//!   `max_chars_per_line` — sentence boundaries are ignored, which is the
//!   common "TikTok-style" continuous-caption look.
//!
//! Both strategies share the same word-wrapping core
//! (`wrap_words_into_captions`), so `max_words_per_line`/`max_chars_per_line`
//! behave identically either way.
//!
//! `Caption::track_id` is left as `String::new()` here — this function has
//! no idea which track the captions are destined for (that's a project/UI
//! concern); the caller (`commands::captions::generate_captions`) fills in
//! the real track id on every returned `Caption` before inserting them into
//! a project via `Command::InsertCaption`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{Caption, TranscriptEntry, Word};

/// Serde/specta-typed (not just an internal Rust enum) so it can cross the
/// Tauri IPC boundary directly as a `commands::captions::generate_captions`
/// parameter, the same way `render::presets::RenderSettings` is typed for
/// `commands::render::start_render_job` despite not being part of
/// `ProjectV1` either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CaptionGroupingMode {
    /// One caption per transcript entry (further wrapped only if the entry
    /// itself exceeds the line limits).
    Sentence,
    /// A single continuous word stream across every entry, chunked strictly
    /// by the line limits, ignoring sentence boundaries.
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CaptionGenerationSettings {
    /// Maximum words on one caption's line. Values `< 1` are treated as `1`
    /// (a defensive floor, not an error — generation always produces
    /// *something* rather than failing on a degenerate settings value).
    pub max_words_per_line: usize,
    /// Maximum characters (Unicode scalar count, not bytes) on one caption's
    /// line, joined-with-single-spaces. Same `< 1` floor as above.
    pub max_chars_per_line: usize,
    pub grouping: CaptionGroupingMode,
}

/// Builds `Vec<Caption>` from `entries`. Assumes `entries` (and each entry's
/// `words`) are already in time order — the normal shape transcription
/// output takes (`project::types::TranscriptEntry` doc comment) — and does
/// not itself re-sort; captions come out in the same relative time order.
pub fn generate_captions_from_transcript(
    entries: &[TranscriptEntry],
    settings: &CaptionGenerationSettings,
) -> Vec<Caption> {
    match settings.grouping {
        CaptionGroupingMode::Sentence => entries
            .iter()
            .flat_map(|entry| wrap_entry_into_captions(entry, settings))
            .collect(),
        CaptionGroupingMode::Word => {
            let all_words: Vec<Word> = entries.iter().flat_map(|e| e.words.clone()).collect();
            if all_words.is_empty() {
                // No provider gave us word-level timing for any entry —
                // fall back to one caption per entry, same honest
                // "don't fabricate word timestamps" policy as the
                // zero-words sentence-mode case below.
                return entries.iter().filter_map(entry_as_single_caption).collect();
            }
            wrap_words_into_captions(&all_words, settings)
        }
    }
}

/// One `TranscriptEntry` -> one or more `Caption`s. When the entry has no
/// per-word timing (a provider that only reports segment-level timing —
/// `TranscriptEntry::words` doc comment), it's emitted as a single caption
/// spanning the whole entry with an empty `words` list, rather than
/// fabricating plausible-looking-but-wrong per-word timestamps by splitting
/// its duration evenly across characters/words.
fn wrap_entry_into_captions(
    entry: &TranscriptEntry,
    settings: &CaptionGenerationSettings,
) -> Vec<Caption> {
    if entry.words.is_empty() {
        return entry_as_single_caption(entry).into_iter().collect();
    }
    wrap_words_into_captions(&entry.words, settings)
}

fn entry_as_single_caption(entry: &TranscriptEntry) -> Option<Caption> {
    if entry.text.trim().is_empty() {
        return None;
    }
    Some(Caption {
        id: uuid::Uuid::new_v4().to_string(),
        track_id: String::new(),
        start_us: entry.start_us,
        end_us: entry.end_us,
        text: entry.text.clone(),
        words: Vec::new(),
        style_id: None,
    })
}

/// Core wrapping algorithm shared by both grouping modes: greedily packs
/// `words` into lines, flushing the current line and starting a new one the
/// moment adding the next word would exceed either limit. A single word
/// that's already longer than `max_chars_per_line` on its own can't be
/// split further, so it's flushed immediately as its own one-word caption
/// instead of waiting forever for a line boundary that will never come.
fn wrap_words_into_captions(words: &[Word], settings: &CaptionGenerationSettings) -> Vec<Caption> {
    let max_words = settings.max_words_per_line.max(1);
    let max_chars = settings.max_chars_per_line.max(1);

    let mut captions = Vec::new();
    let mut current: Vec<Word> = Vec::new();

    for word in words {
        let word_len = word.text.chars().count();

        if !current.is_empty() {
            let projected_len = line_text_len(&current) + 1 + word_len;
            if current.len() + 1 > max_words || projected_len > max_chars {
                captions.push(caption_from_words(std::mem::take(&mut current)));
            }
        }
        current.push(word.clone());

        if current.len() == 1 && word_len > max_chars {
            captions.push(caption_from_words(std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        captions.push(caption_from_words(current));
    }
    captions
}

fn line_text_len(words: &[Word]) -> usize {
    words.iter().map(|w| w.text.chars().count()).sum::<usize>() + words.len().saturating_sub(1)
}

fn caption_from_words(words: Vec<Word>) -> Caption {
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let start_us = words.first().map(|w| w.start_us).unwrap_or(0);
    let end_us = words.last().map(|w| w.end_us).unwrap_or(0);
    Caption {
        id: uuid::Uuid::new_v4().to_string(),
        track_id: String::new(),
        start_us,
        end_us,
        text,
        words,
        style_id: None,
    }
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

    fn settings(
        max_words: usize,
        max_chars: usize,
        grouping: CaptionGroupingMode,
    ) -> CaptionGenerationSettings {
        CaptionGenerationSettings {
            max_words_per_line: max_words,
            max_chars_per_line: max_chars,
            grouping,
        }
    }

    #[test]
    fn sentence_mode_produces_one_caption_per_entry_when_within_limits() {
        let words = vec![word("hello", 0, 500_000), word("world", 500_000, 1_000_000)];
        let entries = vec![entry("e1", "hello world", 0, 1_000_000, words)];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(10, 100, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].text, "hello world");
        assert_eq!(caps[0].start_us, 0);
        assert_eq!(caps[0].end_us, 1_000_000);
        assert_eq!(caps[0].words.len(), 2);
    }

    #[test]
    fn max_words_per_line_wraps_into_multiple_captions() {
        let words = vec![
            word("one", 0, 100),
            word("two", 100, 200),
            word("three", 200, 300),
            word("four", 300, 400),
        ];
        let entries = vec![entry("e1", "one two three four", 0, 400, words)];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(2, 1000, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "one two");
        assert_eq!(caps[0].words.len(), 2);
        assert_eq!(caps[1].text, "three four");
        assert_eq!(caps[1].words.len(), 2);
        // Continuous time coverage across the wrap boundary.
        assert_eq!(caps[0].end_us, 200);
        assert_eq!(caps[1].start_us, 200);
    }

    #[test]
    fn max_chars_per_line_wraps_before_max_words_would() {
        let words = vec![
            word("alpha", 0, 100),
            word("beta", 100, 200),
            word("gamma", 200, 300),
        ];
        let entries = vec![entry("e1", "alpha beta gamma", 0, 300, words)];
        // max_words is generous (10) but max_chars (10) forces a wrap:
        // "alpha beta" is 10 chars, adding " gamma" would be 16.
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(10, 10, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "alpha beta");
        assert_eq!(caps[1].text, "gamma");
    }

    #[test]
    fn single_word_longer_than_max_chars_is_its_own_caption() {
        let words = vec![
            word("supercalifragilisticexpialidocious", 0, 1000),
            word("ok", 1000, 1200),
        ];
        let entries = vec![entry(
            "e1",
            "supercalifragilisticexpialidocious ok",
            0,
            1200,
            words,
        )];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(10, 5, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "supercalifragilisticexpialidocious");
        assert_eq!(caps[0].words.len(), 1);
        assert_eq!(caps[1].text, "ok");
    }

    #[test]
    fn entry_with_zero_words_and_text_falls_back_to_one_caption_with_no_word_timing() {
        let entries = vec![entry("e1", "segment level only", 0, 2_000_000, Vec::new())];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(2, 5, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].text, "segment level only");
        assert_eq!(caps[0].start_us, 0);
        assert_eq!(caps[0].end_us, 2_000_000);
        assert!(caps[0].words.is_empty());
    }

    #[test]
    fn entry_with_zero_words_and_empty_text_produces_no_caption() {
        let entries = vec![entry("e1", "   ", 0, 1_000_000, Vec::new())];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(2, 5, CaptionGroupingMode::Sentence),
        );
        assert!(caps.is_empty());
    }

    #[test]
    fn generation_across_multiple_entries_keeps_them_separate_in_sentence_mode() {
        let e1 = entry("e1", "first", 0, 500_000, vec![word("first", 0, 500_000)]);
        let e2 = entry(
            "e2",
            "second",
            600_000,
            1_100_000,
            vec![word("second", 600_000, 1_100_000)],
        );
        let caps = generate_captions_from_transcript(
            &[e1, e2],
            &settings(10, 100, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "first");
        assert_eq!(caps[1].text, "second");
        assert_eq!(caps[1].start_us, 600_000);
    }

    #[test]
    fn word_mode_flows_continuously_across_entry_boundaries() {
        let e1 = entry(
            "e1",
            "one two",
            0,
            200,
            vec![word("one", 0, 100), word("two", 100, 200)],
        );
        let e2 = entry(
            "e2",
            "three four",
            200,
            400,
            vec![word("three", 200, 300), word("four", 300, 400)],
        );
        // max_words = 3 forces a wrap point that straddles the entry
        // boundary ("one two three" / "four"), proving word mode ignores
        // sentence structure the way sentence mode (previous tests) does not.
        let caps = generate_captions_from_transcript(
            &[e1, e2],
            &settings(3, 1000, CaptionGroupingMode::Word),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "one two three");
        assert_eq!(caps[1].text, "four");
    }

    #[test]
    fn word_mode_with_no_word_level_timing_anywhere_falls_back_per_entry() {
        let e1 = entry("e1", "hello", 0, 100_000, Vec::new());
        let e2 = entry("e2", "world", 100_000, 200_000, Vec::new());
        let caps = generate_captions_from_transcript(
            &[e1, e2],
            &settings(10, 100, CaptionGroupingMode::Word),
        );
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].text, "hello");
        assert_eq!(caps[1].text, "world");
    }

    #[test]
    fn empty_entries_slice_produces_no_captions() {
        let caps =
            generate_captions_from_transcript(&[], &settings(5, 20, CaptionGroupingMode::Sentence));
        assert!(caps.is_empty());
    }

    #[test]
    fn zero_settings_are_floored_to_one_rather_than_panicking() {
        let words = vec![word("a", 0, 100), word("b", 100, 200)];
        let entries = vec![entry("e1", "a b", 0, 200, words)];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(0, 0, CaptionGroupingMode::Sentence),
        );
        // Each word ends up on its own line (limits floored to 1).
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn generated_words_are_time_ordered_within_a_caption() {
        let words = vec![word("a", 0, 100), word("b", 100, 250), word("c", 250, 400)];
        let entries = vec![entry("e1", "a b c", 0, 400, words)];
        let caps = generate_captions_from_transcript(
            &entries,
            &settings(10, 100, CaptionGroupingMode::Sentence),
        );
        assert_eq!(caps.len(), 1);
        let starts: Vec<i64> = caps[0].words.iter().map(|w| w.start_us).collect();
        let mut sorted = starts.clone();
        sorted.sort();
        assert_eq!(
            starts, sorted,
            "words must already be time-ordered for O(log n) active-word lookup"
        );
    }
}
