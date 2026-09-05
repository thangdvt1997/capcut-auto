//! Real FFmpeg audio-filter building blocks for master prompt §38's audio
//! features: a pluggable noise-reduction "architecture"
//! ([`NoiseReductionProvider`]) with one real, simple, working
//! implementation ([`FfmpegNoiseReductionProvider`]), plus the real
//! amplitude-envelope ducking filter chain ([`ducking_filter_chain`]).
//! `render::plan` is the only caller — kept in its own module so the plan
//! builder's already-large per-clip filter-chain logic doesn't grow a second
//! unrelated concern inline.
//!
//! ## Noise reduction: why a trait at all
//!
//! The master prompt explicitly asks for a noise reduction *architecture*,
//! not necessarily a perfectly-tuned filter (module doc comment of the
//! overall Phase 11 brief) — mirroring this codebase's other swappable
//! `*Provider` abstractions (`ai::provider::AIProvider`,
//! `vad::provider::VadProvider`, `transcription::provider::TranscriptionProvider`):
//! today there is exactly one real implementation (a plain FFmpeg filter
//! chain), but a future ML-based denoiser (e.g. FFmpeg's own `arnndn` RNNoise
//! filter, or an out-of-process model) can be swapped in later without
//! touching any call site.
//!
//! ## Ducking: why an amplitude-envelope approach over `sidechaincompress`
//!
//! FFmpeg's real `sidechaincompress` filter needs a second, continuously
//! playing *audio* stream as its sidechain key input — this codebase already
//! has a real, better-suited speech-presence signal instead (`vad::provider::
//! SpeechSegment`s, already-detected discrete time ranges), so building an
//! explicit amplitude envelope directly from those real segments is both
//! more tractable to generate correctly in this codebase's argument-array
//! filter-graph style (`docs/architecture.md`'s "no shell string concat")
//! and more precise (`duck_level`/`attack_us`/`release_us` map onto exact,
//! real timestamps instead of a compressor's threshold/ratio/knee tuning).
//! Real ffmpeg syntax (`volume` filter's `eval=frame` + `enable='between(t,
//! a,b)'` + the `t` time variable) verified against a real ffmpeg build
//! before landing this — see `IMPLEMENTATION_PLAN.md` Phase 11 writeup.

use crate::project::DuckingSettings;
use crate::vad::provider::SpeechSegment;

/// Pluggable noise-reduction backend (module doc comment). Returns the
/// ffmpeg filter-chain fragments (each a complete `name=opts` filter
/// descriptor, comma-joined by the caller alongside every other per-clip
/// audio filter stage) implementing this provider's technique — never a raw
/// shell string, consistent with `render::plan`'s existing argument-array
/// discipline.
pub trait NoiseReductionProvider: Send + Sync {
    /// Human-readable name, surfaced to the frontend/diagnostics.
    fn name(&self) -> &'static str;

    /// The real ffmpeg filter stage(s) implementing this provider's noise
    /// reduction technique, in application order.
    fn filter_chain(&self) -> Vec<String>;
}

/// The one real, simple, working implementation (module doc comment): a
/// `highpass` stage (removes sub-80Hz rumble/handling noise a voice signal
/// never legitimately needs) followed by `afftdn` (ffmpeg's real FFT-based
/// denoiser, at its own sensible default noise floor) — both genuinely real
/// ffmpeg filters, verified to parse/run against a real ffmpeg build.
pub struct FfmpegNoiseReductionProvider;

impl NoiseReductionProvider for FfmpegNoiseReductionProvider {
    fn name(&self) -> &'static str {
        "ffmpeg (highpass + afftdn)"
    }

    fn filter_chain(&self) -> Vec<String> {
        vec!["highpass=f=80".to_string(), "afftdn=nf=-25".to_string()]
    }
}

fn secs(us: i64) -> String {
    format!("{:.6}", us as f64 / 1_000_000.0)
}

/// Builds the real ffmpeg filter chain implementing "duck this clip's volume
/// while speech exists elsewhere" (master prompt §38's auto-duck) for every
/// real `SpeechSegment` overlapping this clip's own
/// `[clip_position_us, clip_position_us + clip_duration_us)` on-timeline
/// window. Three `volume` filter stages per overlapping segment — attack
/// ramp, plateau, release ramp — expressed in ABSOLUTE timeline seconds
/// (`t`), since this chain is meant to run *after* `adelay` has already
/// shifted the clip's samples onto the shared output timeline (module doc
/// comment / `render::plan` call site). Each stage's `enable` window is
/// disjoint from the others' for any real-world segment list where
/// `attack_us`/`release_us` are short relative to the gap between
/// sentences, so ffmpeg evaluates each stage as identity (no-op) outside its
/// own window and the composed effect is exactly "duck during speech, ramp
/// in over `attack_us`, ramp out over `release_us`, unity gain otherwise".
pub fn ducking_filter_chain(
    clip_position_us: i64,
    clip_duration_us: i64,
    voice_speech_segments: &[SpeechSegment],
    ducking: &DuckingSettings,
) -> Vec<String> {
    let clip_start = clip_position_us;
    let clip_end = clip_position_us + clip_duration_us.max(0);
    let duck = ducking.duck_level.clamp(0.0, 1.0);
    let attack_us = ducking.attack_us.max(1);
    let release_us = ducking.release_us.max(1);

    let mut filters = Vec::new();
    for seg in voice_speech_segments {
        let s = seg.start_us.max(clip_start);
        let e = seg.end_us.min(clip_end);
        if e <= s {
            continue; // no overlap with this clip's own on-timeline window
        }

        let attack_end = s + attack_us;
        let release_end = e + release_us;

        // Attack ramp: 1.0 -> duck_level over [s, attack_end].
        filters.push(format!(
            "volume=eval=frame:enable='between(t,{start},{attack_end})':volume='1+({duck}-1)*(t-{start})/{attack}'",
            start = secs(s),
            attack_end = secs(attack_end),
            duck = duck,
            attack = secs(attack_us),
        ));
        // Plateau: held at duck_level for whatever remains of the real
        // speech segment past the attack ramp (a segment shorter than the
        // attack itself simply gets no plateau stage — `attack_end >= e`).
        if attack_end < e {
            filters.push(format!(
                "volume=volume={duck}:enable='between(t,{start},{end})'",
                start = secs(attack_end),
                end = secs(e),
                duck = duck,
            ));
        }
        // Release ramp: duck_level -> 1.0 over [e, release_end].
        filters.push(format!(
            "volume=eval=frame:enable='between(t,{start},{release_end})':volume='{duck}+(1-{duck})*(t-{start})/{release}'",
            start = secs(e),
            release_end = secs(release_end),
            duck = duck,
            release = secs(release_us),
        ));
    }
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_noise_reduction_provider_returns_the_documented_filter_chain() {
        let provider = FfmpegNoiseReductionProvider;
        let chain = provider.filter_chain();
        assert_eq!(
            chain,
            vec!["highpass=f=80".to_string(), "afftdn=nf=-25".to_string()]
        );
        assert!(provider.name().contains("ffmpeg"));
    }

    fn segment(start_us: i64, end_us: i64) -> SpeechSegment {
        SpeechSegment {
            start_us,
            end_us,
            confidence: 0.9,
        }
    }

    fn ducking(duck_level: f64, attack_us: i64, release_us: i64) -> DuckingSettings {
        DuckingSettings {
            duck_level,
            attack_us,
            release_us,
        }
    }

    #[test]
    fn no_overlapping_speech_produces_no_filters() {
        let segments = vec![segment(100_000_000, 101_000_000)];
        let filters =
            ducking_filter_chain(0, 5_000_000, &segments, &ducking(0.25, 100_000, 200_000));
        assert!(filters.is_empty());
    }

    #[test]
    fn one_overlapping_segment_produces_attack_plateau_release() {
        let segments = vec![segment(1_000_000, 3_000_000)];
        let filters =
            ducking_filter_chain(0, 5_000_000, &segments, &ducking(0.25, 100_000, 200_000));
        assert_eq!(filters.len(), 3);
        assert!(filters[0].contains("eval=frame"));
        assert!(filters[0].contains("between(t,1.000000,1.100000)"));
        assert!(filters[1].contains("volume=0.25"));
        assert!(filters[1].contains("between(t,1.100000,3.000000)"));
        assert!(filters[2].contains("between(t,3.000000,3.200000)"));
    }

    #[test]
    fn a_segment_shorter_than_the_attack_skips_the_plateau_stage() {
        // Segment [1.0s, 1.05s) is shorter than the 100ms attack.
        let segments = vec![segment(1_000_000, 1_050_000)];
        let filters =
            ducking_filter_chain(0, 5_000_000, &segments, &ducking(0.25, 100_000, 200_000));
        assert_eq!(filters.len(), 2); // attack + release only, no plateau
    }

    #[test]
    fn segment_is_clamped_to_the_clips_own_window() {
        // Segment starts before the clip and ends after it -> clamped to
        // exactly the clip's own [0, 2_000_000) span.
        let segments = vec![segment(-5_000_000, 50_000_000)];
        let filters =
            ducking_filter_chain(0, 2_000_000, &segments, &ducking(0.25, 100_000, 200_000));
        assert_eq!(filters.len(), 3);
        assert!(filters[0].contains("between(t,0.000000,0.100000)"));
        assert!(filters[2].contains("between(t,2.000000,2.200000)"));
    }

    #[test]
    fn multiple_segments_each_produce_their_own_chain() {
        let segments = vec![segment(1_000_000, 2_000_000), segment(3_000_000, 4_000_000)];
        let filters =
            ducking_filter_chain(0, 5_000_000, &segments, &ducking(0.25, 100_000, 200_000));
        assert_eq!(filters.len(), 6);
    }

    #[test]
    fn duck_level_is_clamped_into_zero_one_range() {
        let segments = vec![segment(1_000_000, 2_000_000)];
        let filters =
            ducking_filter_chain(0, 5_000_000, &segments, &ducking(5.0, 100_000, 200_000));
        assert!(filters.iter().any(|f| f.contains("volume=1")));
    }
}
