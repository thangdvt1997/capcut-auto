//! Wires a validated `Vec<BRollSuggestion>` (`broll::suggest`, the
//! AI-dependent half) to real local candidates (`broll::provider`, the
//! no-AI-needed half) — master prompt §34's own two-stage pipeline: the AI
//! proposes *what* to look for and *when*, then a real search over the local
//! media library finds *what's actually available*. Mirrors
//! `highlights::combine`'s role of being the one place the AI-dependent and
//! local-signal halves of a feature meet, without either half depending on
//! the other directly.

use super::provider::{BRollCandidate, BRollProvider, BRollQuery};
use super::suggest::BRollSuggestion;

/// One AI-proposed suggestion paired with whatever real local B-roll
/// [`BRollProvider::search`] found for its `keyword` — `candidates` may be
/// empty. That's an **honest, expected outcome** (module doc comment's "no
/// local B-roll found for this suggestion" case), never papered over with a
/// fabricated match: a caller (frontend, later pass) can tell the user
/// exactly that and offer to import matching footage instead.
#[derive(Debug, Clone, PartialEq)]
pub struct BRollSuggestionWithCandidates {
    pub suggestion: BRollSuggestion,
    pub candidates: Vec<BRollCandidate>,
}

/// For each suggestion, searches `provider` for local B-roll matching its
/// `keyword`, capping results per suggestion at `candidates_per_suggestion`.
/// A real search failure (`BRollError`, e.g. the underlying db call itself
/// erroring) is **not** allowed to poison the whole batch — the same
/// per-item "one bad entry doesn't fail the rest" shape
/// `commands::media::import_media_paths` uses for multi-file import: a
/// suggestion whose search failed for real gets an empty `candidates` list
/// rather than aborting suggestions after it. This function never talks to
/// an `AIProvider` itself; `suggestions` must already be validated (the
/// caller runs `broll::suggest::parse_and_validate` first).
pub fn suggest_and_search(
    provider: &dyn BRollProvider,
    suggestions: Vec<BRollSuggestion>,
    candidates_per_suggestion: u32,
) -> Vec<BRollSuggestionWithCandidates> {
    suggestions
        .into_iter()
        .map(|suggestion| {
            let query = BRollQuery::new(suggestion.keyword.clone(), candidates_per_suggestion);
            let candidates = provider.search(&query).unwrap_or_default();
            BRollSuggestionWithCandidates {
                suggestion,
                candidates,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broll::error::BRollError;
    use crate::project::MediaKind;

    struct StubProvider {
        by_keyword: std::collections::HashMap<String, Vec<BRollCandidate>>,
    }

    impl BRollProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }
        fn search(&self, query: &BRollQuery) -> Result<Vec<BRollCandidate>, BRollError> {
            Ok(self
                .by_keyword
                .get(&query.keyword)
                .cloned()
                .unwrap_or_default())
        }
    }

    fn candidate(id: &str) -> BRollCandidate {
        BRollCandidate {
            media_id: id.to_string(),
            filename: format!("{id}.mp4"),
            path: format!("/media/{id}.mp4"),
            kind: MediaKind::Video,
            duration_us: 5_000_000,
            width: 1920,
            height: 1080,
            tags: vec![],
            thumbnail_path: None,
        }
    }

    fn suggestion(id: &str, keyword: &str) -> BRollSuggestion {
        BRollSuggestion {
            id: id.to_string(),
            insertion_time_us: 1_000_000,
            duration_us: 2_000_000,
            keyword: keyword.to_string(),
            reason: "because".to_string(),
        }
    }

    #[test]
    fn pairs_each_suggestion_with_matching_local_candidates() {
        let mut by_keyword = std::collections::HashMap::new();
        by_keyword.insert("bitcoin".to_string(), vec![candidate("m1")]);
        let provider = StubProvider { by_keyword };

        let suggestions = vec![suggestion("s1", "bitcoin")];
        let paired = suggest_and_search(&provider, suggestions, 5);

        assert_eq!(paired.len(), 1);
        assert_eq!(paired[0].suggestion.id, "s1");
        assert_eq!(paired[0].candidates.len(), 1);
        assert_eq!(paired[0].candidates[0].media_id, "m1");
    }

    #[test]
    fn a_suggestion_with_no_local_matches_is_an_honest_empty_result_not_an_error() {
        let provider = StubProvider {
            by_keyword: std::collections::HashMap::new(),
        };
        let suggestions = vec![suggestion("s1", "spaceship")];
        let paired = suggest_and_search(&provider, suggestions, 5);

        assert_eq!(paired.len(), 1);
        assert!(paired[0].candidates.is_empty());
    }

    #[test]
    fn multiple_suggestions_are_each_searched_independently() {
        let mut by_keyword = std::collections::HashMap::new();
        by_keyword.insert("bitcoin".to_string(), vec![candidate("m1")]);
        by_keyword.insert(
            "city skyline".to_string(),
            vec![candidate("m2"), candidate("m3")],
        );
        let provider = StubProvider { by_keyword };

        let suggestions = vec![
            suggestion("s1", "bitcoin"),
            suggestion("s2", "city skyline"),
            suggestion("s3", "unrelated"),
        ];
        let paired = suggest_and_search(&provider, suggestions, 5);

        assert_eq!(paired.len(), 3);
        assert_eq!(paired[0].candidates.len(), 1);
        assert_eq!(paired[1].candidates.len(), 2);
        assert!(paired[2].candidates.is_empty());
    }

    struct FailingProvider;
    impl BRollProvider for FailingProvider {
        fn name(&self) -> &'static str {
            "failing"
        }
        fn search(&self, _query: &BRollQuery) -> Result<Vec<BRollCandidate>, BRollError> {
            Err(BRollError::SearchFailed {
                details: "db exploded".to_string(),
            })
        }
    }

    #[test]
    fn a_real_search_failure_degrades_to_an_empty_result_rather_than_aborting_the_batch() {
        let suggestions = vec![suggestion("s1", "bitcoin"), suggestion("s2", "city")];
        let paired = suggest_and_search(&FailingProvider, suggestions, 5);
        assert_eq!(paired.len(), 2);
        assert!(paired[0].candidates.is_empty());
        assert!(paired[1].candidates.is_empty());
    }
}
