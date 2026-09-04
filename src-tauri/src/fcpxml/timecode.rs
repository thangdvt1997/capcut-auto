//! Rational-timecode math for FCPXML export.
//!
//! Reimplemented (design, not code) from `vendor/autocut/src-tauri/src/timecode.rs`'s
//! `detect_rate`/`seconds_to_rational`. Two differences from that module:
//!
//! - This project's native timebase is `i64` microseconds
//!   (`docs/architecture-audit.md` §1/§5), never `f64` seconds and never a raw
//!   SMPTE string — nothing in `ProjectV1` stores an embedded source
//!   timecode, so autocut's `parse_smpte` has no equivalent here. Every
//!   FCPXML clip in this export is therefore stamped `tcFormat="NDF"`
//!   unconditionally; there is no drop-frame *source* timecode to honor.
//! - Frame rates arrive as this project's `Rational` type (`num`/`den`,
//!   `project::types::Rational`), not a raw `f64`. `detect_rate` takes a
//!   `Rational` and converts to `f64` internally only where the NTSC
//!   rate-family comparison needs it — matching autocut's own approach,
//!   which does the same arithmetic in f64 despite taking a nominally exact
//!   input.
//!
//! `us_to_rational` is the `i64`-microseconds equivalent of autocut's
//! `seconds_to_rational`: snaps to the nearest frame at a given
//! [`TimecodeRate`] and renders the FCPXML rational string, keeping the
//! canonical denominator (e.g. `30000` for 29.97, `24000` for 23.976)
//! un-reduced — NLEs key on these well-known values, so simplifying the
//! fraction (e.g. to `1/30`) would be technically equal but not what a real
//! FCPXML importer expects to see.

use crate::project::Rational;

/// One frame, expressed as a `frame_duration_num / frame_duration_den`
/// seconds fraction.
#[derive(Debug, Clone, Copy)]
pub struct TimecodeRate {
    pub nominal_fps: u32,
    pub frame_duration_num: u64,
    pub frame_duration_den: u64,
}

impl TimecodeRate {
    pub fn frame_seconds(&self) -> f64 {
        self.frame_duration_num as f64 / self.frame_duration_den as f64
    }
}

const NTSC_RATES: &[(u32, u64, u64)] = &[
    (24, 1001, 24000), // 23.976
    (30, 1001, 30000), // 29.97
    (60, 1001, 60000), // 59.94
];

/// Stand-in when the caller hands us something that isn't a usable frame
/// rate. Matches `autocut::timecode::FALLBACK_FPS`.
const FALLBACK_FPS: f64 = 30.0;

/// Detect the canonical [`TimecodeRate`] for a project/media frame rate.
///
/// `fps.den == 0` (a malformed `Rational`) is treated the same as a
/// zero/negative/non-finite fps: it falls back to `FALLBACK_FPS` rather than
/// dividing by zero.
pub fn detect_rate(fps: Rational) -> TimecodeRate {
    let raw = if fps.den == 0 {
        0.0
    } else {
        fps.num as f64 / fps.den as f64
    };
    detect_rate_from_f64(raw)
}

/// The actual guarded rate-detection logic, taking a raw `f64` so the
/// degenerate-input guard (ported from autocut's regression-tested edge
/// cases: zero, negative, NaN, infinity) can be exercised directly in tests
/// without having to first contort a `Rational` into representing them
/// (`Rational`'s fields are unsigned, so negative/NaN/infinite fps aren't
/// even representable through the public `detect_rate` entry point — the
/// guard is kept anyway because this is a direct port of autocut's proven
/// logic, and `fps.den == 0` alone can still reach the zero/degenerate path
/// through `detect_rate` above).
pub(crate) fn detect_rate_from_f64(fps: f64) -> TimecodeRate {
    // Zero, negative, NaN and infinity all have to be turned away here. The
    // rational fallback at the bottom of this function computes
    // `(120_000.0 / fps).round() as u64`, and Rust saturates out-of-range
    // float casts, so fps <= 0 (or NaN/infinite) would otherwise yield a
    // frame_duration numerator of u64::MAX — one "frame" lasting about 4.8
    // million years. Everything downstream then rounds to zero frames and
    // the FCPXML exports an empty timeline (see
    // `us_to_rational_still_measures_time_at_a_degenerate_fps` below).
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        FALLBACK_FPS
    };
    let nominal = fps.round() as i64;
    if nominal > 0 {
        if let Some(&(n, num, den)) = NTSC_RATES.iter().find(|(n, _, _)| *n as i64 == nominal) {
            let ntsc = n as f64 * 1000.0 / 1001.0;
            if (fps - ntsc).abs() < 0.01 {
                return TimecodeRate {
                    nominal_fps: n,
                    frame_duration_num: num,
                    frame_duration_den: den,
                };
            }
        }
    }
    if nominal > 0 && (fps - nominal as f64).abs() < 0.001 {
        return TimecodeRate {
            nominal_fps: nominal as u32,
            frame_duration_num: 1,
            frame_duration_den: nominal as u64,
        };
    }
    // Fallback: approximate with a 120_000-denominator rational. Rare path —
    // an odd, non-NTSC, non-integer frame rate.
    let den = 120_000u64;
    let num = ((den as f64 / fps).round() as u64).max(1);
    TimecodeRate {
        nominal_fps: nominal.max(1) as u32,
        frame_duration_num: num,
        frame_duration_den: den,
    }
}

/// Render `us` microseconds, snapped to the nearest frame at `rate`, as a
/// FCPXML rational string (e.g. `"30030/30000s"`). `us <= 0` renders `"0s"`,
/// matching autocut's zero-duration edge case.
pub fn us_to_rational(us: i64, rate: &TimecodeRate) -> String {
    if us <= 0 {
        return "0s".to_string();
    }
    let seconds = us as f64 / 1_000_000.0;
    let frames = (seconds / rate.frame_seconds()).round() as u64;
    let numerator = frames * rate.frame_duration_num;
    format!("{}/{}s", numerator, rate.frame_duration_den)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rate_ntsc_2997() {
        let r = detect_rate(Rational::new(30000, 1001));
        assert_eq!(r.nominal_fps, 30);
        assert_eq!(r.frame_duration_num, 1001);
        assert_eq!(r.frame_duration_den, 30000);
    }

    #[test]
    fn detect_rate_integer_25() {
        let r = detect_rate(Rational::new(25, 1));
        assert_eq!(r.nominal_fps, 25);
        assert_eq!(r.frame_duration_num, 1);
        assert_eq!(r.frame_duration_den, 25);
    }

    #[test]
    fn detect_rate_zero_denominator_falls_back_safely() {
        // A malformed Rational (den=0) must not panic (division by zero) or
        // produce a degenerate rate.
        let r = detect_rate(Rational::new(30, 0));
        assert!(r.frame_seconds() > 0.0 && r.frame_seconds() <= 1.0);
        assert!(r.nominal_fps > 0);
    }

    #[test]
    fn us_to_rational_2997() {
        let r = detect_rate(Rational::new(30000, 1001));
        // 1 second at 29.97 NTSC is 30 frames * 1001/30000 = 30030/30000 s.
        let s = us_to_rational(1_000_000, &r);
        assert_eq!(s, "30030/30000s");
    }

    #[test]
    fn us_to_rational_zero() {
        let r = detect_rate(Rational::new(30, 1));
        assert_eq!(us_to_rational(0, &r), "0s");
        assert_eq!(us_to_rational(-5, &r), "0s");
    }

    #[test]
    fn detect_rate_refuses_to_build_a_degenerate_rate() {
        // Ported scenario from autocut's timecode.rs test module: a
        // non-positive/non-finite fps used to reach the 120_000-denominator
        // fallback and compute `(120_000.0 / 0.0).round() as u64`, which
        // saturates to u64::MAX — one "frame" ~1.5e9 seconds long.
        for fps in [0.0, -12.0, f64::NAN, f64::INFINITY] {
            let r = detect_rate_from_f64(fps);
            assert!(
                r.frame_seconds() > 0.0 && r.frame_seconds() <= 1.0,
                "fps {fps} produced frame_seconds {}",
                r.frame_seconds()
            );
            assert!(r.nominal_fps > 0, "fps {fps} produced nominal 0");
        }
    }

    #[test]
    fn us_to_rational_still_measures_time_at_a_degenerate_fps() {
        // Downstream of the bug above: one second rounded to zero frames and
        // every asset-clip in the exported FCPXML carried a zero duration,
        // so the NLE imported an empty timeline. Assert on the numerator
        // rather than the string: the degenerate path renders "0/120000s",
        // just as empty as "0s" but not equal to it.
        let r = detect_rate_from_f64(0.0);
        let rendered = us_to_rational(1_000_000, &r);
        let numerator: u64 = rendered
            .trim_end_matches('s')
            .split('/')
            .next()
            .expect("rational always has a numerator")
            .parse()
            .expect("numerator is an integer");
        assert!(numerator > 0, "one second rendered as {rendered}");
    }
}
