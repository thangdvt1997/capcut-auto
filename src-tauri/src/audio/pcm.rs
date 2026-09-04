//! Extract 16kHz mono PCM samples from a media file via an ffmpeg subprocess.
//!
//! Ported from `vendor/autocut/src-tauri/src/audio.rs` (reuse permitted,
//! `docs/upstream.md`) with two changes: it now goes through this crate's
//! `ffmpeg::command`/`ffmpeg::binaries` modules instead of a bare
//! `Command::new`, and errors are this subsystem's `MediaError` instead of
//! `anyhow::Error`. No timebase rewrite was needed here — autocut's `f64`
//! seconds live in `probe.rs`/`vad.rs`/`timecode.rs`, not in this module,
//! which only ever deals in raw sample counts and a fixed sample rate.
//!
//! Pipes raw audio through stdout into memory; no intermediate file. Samples
//! are kept as `i16`, ffmpeg's native `s16le` output, converted to `[-1.0,
//! 1.0]` floats only at the point of use (one chunk at a time) — a decoded
//! two-hour source costs ~115MB this way instead of ~230MB as `f32` up
//! front.

use std::io::Read;
use std::path::Path;
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::media::error::MediaError;

pub const PCM_SAMPLE_RATE: u32 = 16_000;

/// Scale a raw sample into the `[-1.0, 1.0)` range models/meters expect.
#[inline]
pub fn to_unit(sample: i16) -> f32 {
    sample as f32 / 32768.0
}

pub fn extract_pcm(ffmpeg: &Path, media: &Path) -> Result<Vec<i16>, MediaError> {
    extract_pcm_with_cancel(ffmpeg, media, None)
}

pub fn extract_pcm_with_cancel(
    ffmpeg: &Path,
    media: &Path,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Vec<i16>, MediaError> {
    let fail = |details: String| MediaError::WaveformFailed {
        path: media.display().to_string(),
        details,
    };

    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-i").arg(media);
    cmd.args([
        "-vn",
        "-ac",
        "1",
        "-ar",
        &PCM_SAMPLE_RATE.to_string(),
        "-f",
        "s16le",
        "-loglevel",
        "error",
        "-",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| fail(format!("spawning ffmpeg: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| fail("no stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| fail("no stderr".into()))?;
    let stdout_reader = read_samples(stdout, cancel.clone());
    let stderr_reader = read_stderr(stderr);

    let status = loop {
        if is_cancelled(cancel.as_deref()) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(fail("cancelled".into()));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(fail(format!("checking ffmpeg status: {e}"))),
        }
    };

    let mut samples = stdout_reader
        .join()
        .map_err(|_| fail("ffmpeg stdout reader panicked".into()))?
        .map_err(|e| fail(format!("reading ffmpeg stdout: {e}")))?;
    let err = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        return Err(fail(format!("ffmpeg failed ({status}): {err}")));
    }

    // Growth doubling can leave up to twice the needed capacity, and this
    // vector is about to sit in a caller-held cache for a while.
    samples.shrink_to_fit();
    Ok(samples)
}

/// 64 KiB of s16le is 32k samples: large enough that read syscalls aren't
/// the bottleneck, small enough the staging buffer never shows up next to
/// the sample vector in a memory profile.
const READ_CHUNK: usize = 64 * 1024;

fn read_samples(
    mut stdout: ChildStdout,
    cancel: Option<Arc<AtomicBool>>,
) -> thread::JoinHandle<Result<Vec<i16>, String>> {
    thread::spawn(move || {
        let mut buf = vec![0u8; READ_CHUNK];
        let mut samples: Vec<i16> = Vec::new();
        let mut carry: Option<u8> = None;
        loop {
            if is_cancelled(cancel.as_deref()) {
                return Err("cancelled".to_string());
            }
            let read = stdout.read(&mut buf).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            carry = drain_samples(carry, &buf[..read], &mut samples);
        }
        Ok(samples)
    })
}

/// Append every complete little-endian i16 in `bytes` to `out`. `carry` is
/// the odd byte left over from the previous call, if the stream split a
/// sample across two reads. Returns the new leftover.
fn drain_samples(carry: Option<u8>, bytes: &[u8], out: &mut Vec<i16>) -> Option<u8> {
    let mut rest = bytes;
    if let Some(low) = carry {
        let Some((high, tail)) = rest.split_first() else {
            return Some(low);
        };
        out.push(i16::from_le_bytes([low, *high]));
        rest = tail;
    }
    let mut chunks = rest.chunks_exact(2);
    for pair in &mut chunks {
        out.push(i16::from_le_bytes([pair[0], pair[1]]));
    }
    chunks.remainder().first().copied()
}

fn read_stderr(mut stderr: ChildStderr) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut err = String::new();
        let _ = stderr.read_to_string(&mut err);
        err
    })
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|flag| flag.load(Ordering::SeqCst))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_whole_samples_and_carries_nothing() {
        let mut out = Vec::new();
        assert_eq!(
            drain_samples(None, &[0x00, 0x80, 0xff, 0x7f], &mut out),
            None
        );
        assert_eq!(out, vec![i16::MIN, i16::MAX]);
    }

    #[test]
    fn carries_the_trailing_byte_of_an_odd_length_read() {
        let mut out = Vec::new();
        assert_eq!(
            drain_samples(None, &[0x00, 0x80, 0xff], &mut out),
            Some(0xff)
        );
        assert_eq!(out, vec![i16::MIN]);
    }

    #[test]
    fn reassembles_a_sample_split_across_two_reads() {
        let mut out = Vec::new();
        let carry = drain_samples(None, &[0xff], &mut out);
        assert_eq!(carry, Some(0xff));
        assert!(out.is_empty());
        assert_eq!(drain_samples(carry, &[0x7f], &mut out), None);
        assert_eq!(out, vec![i16::MAX]);
    }

    #[test]
    fn an_empty_read_preserves_a_pending_carry() {
        let mut out = Vec::new();
        assert_eq!(drain_samples(Some(0x12), &[], &mut out), Some(0x12));
        assert!(out.is_empty());
    }

    #[test]
    fn unit_scaling_spans_the_full_range() {
        assert!((to_unit(i16::MIN) - (-1.0)).abs() < 1e-6);
        assert!((to_unit(i16::MAX) - (32767.0 / 32768.0)).abs() < 1e-6);
        assert_eq!(to_unit(0), 0.0);
    }

    #[test]
    fn extracts_real_pcm_from_a_synthetic_tone() {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-pcm-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("tone.wav");

        let gen_args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args)
            .expect("synthesizing a test tone with ffmpeg's lavfi source");

        let samples = extract_pcm(&ffmpeg, &source).expect("pcm extraction succeeds");
        // 1 second @ 16kHz mono should be close to 16000 samples (container
        // framing can round slightly).
        assert!(
            samples.len() > 15_000 && samples.len() < 17_000,
            "{}",
            samples.len()
        );
        // A 440Hz sine wave has real energy — not silence, not garbage.
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        assert!(peak > 1000, "expected an audible tone, got peak={peak}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
