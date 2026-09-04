//! Filler-word detection (master prompt §16): transcript entries → proposed
//! (`applied: false`) `Cut { reason: FillerWord }` candidates that a caller
//! shows to the user ("show candidates first"), lets them select a subset
//! of ("select all" / "deselect" is naturally frontend selection-state over
//! this candidate list), and applies via the *already-built* apply-side
//! infrastructure: `crate::timeline::silence::apply_cuts_to_clip`/
//! `apply_cuts_to_track` (also exposed as
//! `commands::timeline::apply_silence_cuts`/`apply_silence_cuts_to_track`)
//! don't care *why* a `Cut` exists — they're generic over `Cut::reason` —
//! so this module deliberately does NOT add its own apply-side command:
//! the existing `apply_silence_cuts(_to_track)` commands are reused AS-IS
//! for filler-word cuts too (pass them a `Vec<Cut>` with
//! `reason: FillerWord` and they work unmodified). This mirrors
//! `vad::cutlist`'s module doc comment almost exactly, deliberately — same
//! "detect → pad → merge → emit proposed cuts, apply is somebody else's
//! already-tested job" shape.
//!
//! ## Granularity: entry-level, not word-level
//!
//! `project::types::TranscriptEntry` (as of this writing — checked the live
//! schema before writing this module, since a concurrent whisper-integration
//! work-stream could plausibly have added word-level data to it) carries
//! only a whole-entry `start_us`/`end_us` span, no per-word timestamps.
//! Master prompt §14 prefers word-level timestamps for the transcript
//! itself, and detection *would* use a matched word/phrase's own timestamps
//! if they were available — but faking a sub-entry timestamp by
//! interpolating over character position would produce a falsely precise
//! candidate with no real transcription evidence behind it, which is worse
//! than being honest about the limitation. So: **every candidate `Cut`
//! spans the containing `TranscriptEntry`'s whole `start_us..end_us`**,
//! whenever ANY dictionary phrase matches inside its text.
//!
//! This is a real, stated limitation: if a filler phrase appears in the
//! middle of an otherwise-long sentence, applying that candidate removes
//! the entire sentence's audio, not just the filler phrase. In practice
//! this is least surprising when a transcription provider already segments
//! isolated fillers into their own short entries (common for whisper.cpp on
//! discourse-marker-only utterances), and is exactly why §16 requires
//! showing candidates for preview/select before apply — a caller/UI should
//! let the user read the entry text before applying a candidate whose entry
//! is a long sentence with an embedded filler. Should
//! `TranscriptEntry` (or `Caption`'s existing `Word`) later gain reliable
//! per-word timestamps, a `build_cuts_from_filler_word_spans` sibling
//! function operating on `(word, start_us, end_us)` triples should be added
//! alongside this one and preferred over it — the matching/tokenization
//! logic below (`tokenize`/`contains_phrase`/`text_contains_filler_word`)
//! already operates on plain text and
//! phrase lists, so it is reusable as-is for that future word-level path.
//!
//! ## Padding: shrinks the cut inward, not outward
//!
//! `vad::cutlist::CutParams` (reused here verbatim, both for consistency
//! and so the frontend has one padding-slider vocabulary across both
//! detectors) is defined in terms of expanding *kept* speech regions there.
//! Filler-word detection operates directly on the region to *remove*, so
//! applying the same padding values the same direction (expanding the
//! removed region) would cut MORE surrounding real speech — the opposite of
//! "so speech is not cut unnaturally". Instead, `padding_before_us`/
//! `padding_after_us` here shrink the candidate cut inward from each edge
//! (`start += padding_before_us`, `end -= padding_after_us`, clamped so
//! `start <= end`): the net effect in both modules is identical — a
//! configurable buffer of real speech survives around a detected boundary,
//! it's just that here the boundary being buffered is the cut's own edges
//! rather than a kept-region's edges. A padding pair large enough to
//! consume the whole entry span degenerates to a zero-length cut, which is
//! filtered out (mirrors `build_cuts_from_speech_segments`'s own
//! `filter(|(s, e)| e > s)`). `merge_gap_us` keeps its cutlist meaning of
//! "nearby regions collapse into one", just applied directly to candidate
//! cut intervals (already `Remove` intervals here) rather than to kept
//! regions first.
//!
//! ## Tokenization / word-boundary matching
//!
//! No regex dependency exists in this crate yet (see `Cargo.toml`), and
//! pulling one in for this alone seemed unwarranted, so matching is a
//! small hand-rolled tokenizer: split text into maximal runs of
//! `char::is_alphanumeric` characters (Unicode-aware, so Vietnamese
//! diacritics count as ordinary word characters), lowercase each token
//! (Unicode-aware `to_lowercase`, safe to apply uniformly to Vietnamese
//! text too), then match a dictionary phrase (itself tokenized the same
//! way) as a contiguous run of *whole* tokens via a sliding window. This
//! gives correct word-boundary behavior for English for free: "like" is
//! one token and "unlike"/"likely" are each a different single token, so
//! they never equal the one-token phrase `["like"]`.
//!
//! Vietnamese is conventionally already written with each syllable
//! space-separated (unlike Chinese/Japanese), so this same whitespace/
//! punctuation tokenizer naturally yields one token per syllable — enough
//! to match both the single-syllable defaults (`ờ`, `ừ`, `ừm`, `à`, `ờm`)
//! and the two-syllable phrase `kiểu như` as two adjacent syllable tokens.
//! The known simplification (documented rather than silently assumed): a
//! filler syllable that also happens to be a syllable *within* some other
//! multi-syllable word would still register as a standalone-token match,
//! since this tokenizer has no notion of Vietnamese compound-word
//! boundaries beyond syllable splitting. This is judged an acceptable,
//! reasonable simplification for the default dictionary (real discourse
//! particles, not fragments of other words in ordinary usage).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::project::{Cut, CutKind, CutReason, TranscriptEntry};
use crate::vad::CutParams;

/// English defaults, master prompt §16, verbatim.
pub const DEFAULT_EN_FILLERS: &[&str] = &["uh", "um", "erm", "you know", "like"];

/// Vietnamese defaults, master prompt §16, verbatim.
pub const DEFAULT_VI_FILLERS: &[&str] = &["ờ", "ừ", "ừm", "à", "ờm", "kiểu như"];

/// Custom-dictionary support (§16 "Allow custom dictionary"): a caller
/// supplies additional words/phrases, and independently controls whether
/// the EN+VI built-ins are included at all (additive-by-default, but a
/// caller who wants ONLY their own dictionary can set `use_defaults: false`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct FillerDictionary {
    /// Include `DEFAULT_EN_FILLERS` and `DEFAULT_VI_FILLERS`.
    pub use_defaults: bool,
    /// Additional words/phrases on top of (or, if `use_defaults` is false,
    /// instead of) the built-in defaults. Matched with the same
    /// tokenized/case-insensitive rules as the defaults.
    pub custom_dictionary: Vec<String>,
}

impl FillerDictionary {
    pub fn new(use_defaults: bool, custom_dictionary: Vec<String>) -> Self {
        Self {
            use_defaults,
            custom_dictionary,
        }
    }

    /// The effective phrase list: defaults (if enabled) followed by custom
    /// entries. Empty phrases are dropped (defensive against a caller
    /// passing e.g. `""` from an empty text-input row in a custom-dictionary
    /// editor UI).
    fn phrases(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if self.use_defaults {
            out.extend(DEFAULT_EN_FILLERS.iter().copied());
            out.extend(DEFAULT_VI_FILLERS.iter().copied());
        }
        out.extend(
            self.custom_dictionary
                .iter()
                .map(String::as_str)
                .filter(|s| !s.trim().is_empty()),
        );
        out
    }
}

/// Splits `text` into maximal runs of Unicode-alphanumeric characters,
/// lowercased. See module doc comment ("Tokenization / word-boundary
/// matching") for the reasoning.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// True if `phrase`'s tokens appear as a contiguous, whole-token run
/// anywhere in `tokens`.
fn contains_phrase(tokens: &[String], phrase: &[String]) -> bool {
    if phrase.is_empty() || tokens.len() < phrase.len() {
        return false;
    }
    tokens.windows(phrase.len()).any(|w| w == phrase)
}

/// True if any phrase in `dictionary` matches somewhere in `text`. Exposed
/// standalone (not just inlined into the `Cut`-building loop below) so the
/// tokenization/matching logic is independently testable and directly
/// reusable by a future word-level variant (see module doc comment).
pub fn text_contains_filler_word(text: &str, dictionary: &FillerDictionary) -> bool {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return false;
    }
    dictionary
        .phrases()
        .iter()
        .map(|p| tokenize(p))
        .any(|phrase_tokens| contains_phrase(&tokens, &phrase_tokens))
}

/// Builds the proposed (`applied: false`) `Cut { reason: FillerWord }`
/// candidates for every `TranscriptEntry` in `entries` whose text contains
/// a dictionary match, per `cut_params` (see module doc comment for the
/// shrink-inward padding semantics and entry-level granularity decision).
/// Pure and stateless — cheap enough to re-run on every dictionary/padding
/// change, same design principle as `vad::cutlist::build_cuts_from_speech_segments`.
pub fn build_cuts_from_filler_words(
    entries: &[TranscriptEntry],
    dictionary: &FillerDictionary,
    cut_params: CutParams,
) -> Vec<Cut> {
    // (media_id, start_us, end_us) candidates, pre-merge.
    let mut candidates: Vec<(String, i64, i64)> = entries
        .iter()
        .filter(|entry| text_contains_filler_word(&entry.text, dictionary))
        .map(|entry| {
            let start = (entry.start_us + cut_params.padding_before_us).min(entry.end_us);
            let end = (entry.end_us - cut_params.padding_after_us).max(start);
            (entry.media_id.clone(), start.max(0), end.max(0))
        })
        .filter(|(_, start, end)| end > start)
        .collect();

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let merge_gap = cut_params.merge_gap_us.max(0);
    let mut merged: Vec<(String, i64, i64)> = Vec::with_capacity(candidates.len());
    for (media_id, start, end) in candidates {
        if let Some(last) = merged.last_mut() {
            if last.0 == media_id && start - last.2 <= merge_gap {
                last.2 = last.2.max(end);
                continue;
            }
        }
        merged.push((media_id, start, end));
    }

    merged
        .into_iter()
        .map(|(media_id, start_us, end_us)| Cut {
            id: uuid::Uuid::new_v4().to_string(),
            kind: CutKind::Remove,
            source_media_id: media_id,
            start_us,
            end_us,
            reason: CutReason::FillerWord,
            applied: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, media_id: &str, text: &str, start_us: i64, end_us: i64) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: media_id.to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words: Vec::new(),
            is_filler: false,
        }
    }

    fn defaults() -> FillerDictionary {
        FillerDictionary::new(true, Vec::new())
    }

    fn no_padding() -> CutParams {
        CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 0,
        }
    }

    // -- default dictionary matching -----------------------------------

    #[test]
    fn every_default_english_filler_is_detected() {
        for word in DEFAULT_EN_FILLERS {
            assert!(
                text_contains_filler_word(&format!("well {word} so"), &defaults()),
                "expected {word:?} to match"
            );
        }
    }

    #[test]
    fn every_default_vietnamese_filler_is_detected() {
        for word in DEFAULT_VI_FILLERS {
            assert!(
                text_contains_filler_word(&format!("thì {word} là vậy"), &defaults()),
                "expected {word:?} to match"
            );
        }
    }

    #[test]
    fn multi_word_english_phrase_matches_as_whole_phrase() {
        assert!(text_contains_filler_word(
            "it's, you know, complicated",
            &defaults()
        ));
        // Tokens present but not adjacent must NOT match.
        assert!(!text_contains_filler_word("you don't know", &defaults()));
    }

    #[test]
    fn multi_word_vietnamese_phrase_matches_as_whole_phrase() {
        assert!(text_contains_filler_word("nó kiểu như vậy đó", &defaults()));
        assert!(!text_contains_filler_word("kiểu dáng như vậy", &defaults()));
    }

    // -- word-boundary correctness -------------------------------------

    #[test]
    fn like_matches_standalone_but_not_inside_a_longer_word() {
        assert!(text_contains_filler_word("I like that", &defaults()));
        assert!(!text_contains_filler_word("unlike that", &defaults()));
        assert!(!text_contains_filler_word("it's likely fine", &defaults()));
    }

    // -- case-insensitivity ---------------------------------------------

    #[test]
    fn english_matching_is_case_insensitive() {
        assert!(text_contains_filler_word("UM, wait", &defaults()));
        assert!(text_contains_filler_word(
            "You Know what I mean",
            &defaults()
        ));
    }

    // -- custom dictionary ------------------------------------------------

    #[test]
    fn custom_dictionary_is_additive_by_default() {
        let dict = FillerDictionary::new(true, vec!["basically".to_string()]);
        assert!(text_contains_filler_word("basically it works", &dict));
        assert!(text_contains_filler_word("um it works", &dict)); // default still active
    }

    #[test]
    fn use_defaults_false_excludes_the_built_ins() {
        let dict = FillerDictionary::new(false, vec!["basically".to_string()]);
        assert!(text_contains_filler_word("basically it works", &dict));
        assert!(!text_contains_filler_word("um it works", &dict));
    }

    #[test]
    fn empty_custom_dictionary_entries_are_ignored() {
        let dict = FillerDictionary::new(false, vec!["".to_string(), "   ".to_string()]);
        assert!(!text_contains_filler_word("um anything at all", &dict));
    }

    // -- Cut generation: reason/applied/kind ------------------------------

    #[test]
    fn produced_cuts_have_filler_word_reason_and_are_never_pre_applied() {
        let entries = [entry("e1", "m1", "um hello", 0, 1_000_000)];
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), no_padding());
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].kind, CutKind::Remove);
        assert_eq!(cuts[0].reason, CutReason::FillerWord);
        assert!(!cuts[0].applied);
        assert_eq!(cuts[0].source_media_id, "m1");
    }

    #[test]
    fn entries_without_a_filler_word_produce_no_cuts() {
        let entries = [entry("e1", "m1", "hello there", 0, 1_000_000)];
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), no_padding());
        assert!(cuts.is_empty());
    }

    #[test]
    fn empty_entries_produce_no_cuts() {
        let cuts = build_cuts_from_filler_words(&[], &defaults(), no_padding());
        assert!(cuts.is_empty());
    }

    #[test]
    fn entry_level_granularity_spans_the_whole_entry() {
        let entries = [entry(
            "e1",
            "m1",
            "so anyway, you know, that happened",
            2_000_000,
            5_000_000,
        )];
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), no_padding());
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start_us, 2_000_000);
        assert_eq!(cuts[0].end_us, 5_000_000);
    }

    // -- padding: independent before/after, shrinking the cut inward -----

    #[test]
    fn padding_shrinks_the_cut_inward_independently() {
        let entries = [entry("e1", "m1", "um", 1_000_000, 2_000_000)];
        let params = CutParams {
            padding_before_us: 100_000,
            padding_after_us: 300_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start_us, 1_100_000);
        assert_eq!(cuts[0].end_us, 1_700_000);
    }

    #[test]
    fn padding_that_consumes_the_whole_entry_produces_no_cut() {
        let entries = [entry("e1", "m1", "um", 1_000_000, 1_200_000)];
        let params = CutParams {
            padding_before_us: 100_000,
            padding_after_us: 200_000,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert!(cuts.is_empty());
    }

    #[test]
    fn padding_never_produces_a_negative_start() {
        let entries = [entry("e1", "m1", "um", 0, 200_000)];
        let params = CutParams {
            padding_before_us: -500_000, // caller asking to expand, not shrink
            padding_after_us: 0,
            merge_gap_us: 0,
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start_us, 0);
    }

    // -- merge_gap_us: nearby candidate cuts collapse into one ------------

    #[test]
    fn nearby_candidate_cuts_merge_within_merge_gap() {
        let entries = [
            entry("e1", "m1", "um", 0, 500_000),
            entry("e2", "m1", "like this", 600_000, 1_000_000),
        ];
        let params = CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 200_000, // gap between candidates is 100_000 <= 200_000
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].start_us, 0);
        assert_eq!(cuts[0].end_us, 1_000_000);
    }

    #[test]
    fn distant_candidate_cuts_do_not_merge() {
        let entries = [
            entry("e1", "m1", "um", 0, 500_000),
            entry("e2", "m1", "like this", 900_000, 1_400_000),
        ];
        let params = CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 100_000, // gap is 400_000 > 100_000
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert_eq!(cuts.len(), 2);
    }

    #[test]
    fn candidate_cuts_for_different_media_never_merge() {
        let entries = [
            entry("e1", "m1", "um", 0, 500_000),
            entry("e2", "m2", "um", 500_000, 1_000_000),
        ];
        let params = CutParams {
            padding_before_us: 0,
            padding_after_us: 0,
            merge_gap_us: 1_000_000,
        };
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), params);
        assert_eq!(cuts.len(), 2);
        assert!(cuts.iter().any(|c| c.source_media_id == "m1"));
        assert!(cuts.iter().any(|c| c.source_media_id == "m2"));
    }

    // -- integration: consumed by the already-built apply-side engine -----

    #[test]
    fn produced_cuts_apply_cleanly_through_existing_apply_cuts_to_clip() {
        use crate::project::{ClipSettings, ProjectV1, Track, TrackKind};
        use crate::timeline::ops::clip_span;
        use crate::timeline::silence::apply_cuts_to_clip;

        let mut project = ProjectV1::new("filler apply test");
        project.tracks.push(Track {
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
        project.clips.push(crate::project::Clip {
            id: "c1".into(),
            track_id: "t1".into(),
            media_id: Some("m1".into()),
            source_in_us: 0,
            source_out_us: 10_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });

        let entries = [entry(
            "e1",
            "m1",
            "so um that happened",
            3_000_000,
            5_000_000,
        )];
        let cuts = build_cuts_from_filler_words(&entries, &defaults(), no_padding());
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].reason, CutReason::FillerWord);

        let command = apply_cuts_to_clip(&project, "c1", &cuts).expect("apply should succeed");
        command
            .apply(&mut project)
            .expect("command apply should succeed");

        let mut spans: Vec<(i64, i64)> = project.clips.iter().map(clip_span).collect();
        spans.sort();
        assert_eq!(spans, vec![(0, 3_000_000), (5_000_000, 10_000_000)]);
    }
}
