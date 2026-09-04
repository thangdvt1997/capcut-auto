//! FFmpeg/ffprobe process-argument-array builder (master prompt §66/§88):
//! every caller in this crate that shells out to ffmpeg/ffprobe goes through
//! `FfmpegArgs`, never hand-rolled string concatenation. This is what makes
//! Windows paths with spaces/Unicode/Vietnamese characters (§88's own
//! examples: `D:\My Videos\Test Video.mp4`, `C:\Video tiếng Việt\phỏng vấn
//! 01.mp4`) safe by construction — each argument is a distinct `OsString` in
//! a `Vec`, handed straight to `std::process::Command`, never joined into a
//! shell command line where quoting could be gotten wrong.
//!
//! Deliberately general enough to cover today's callers (probe, thumbnail,
//! proxy, PCM extraction) and Phase 6's render engine later, without
//! over-building for features that don't exist yet — no filter-graph DSL, no
//! preset system here.

use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, Context, Result};

/// An ordered list of ffmpeg/ffprobe arguments, built incrementally.
/// Everything is an `OsString` — never a `String` that gets concatenated —
/// so a filename containing spaces, quotes, or non-ASCII text is carried as
/// one opaque argument all the way to `exec`/`CreateProcess`.
#[derive(Debug, Default, Clone)]
pub struct FfmpegArgs {
    args: Vec<OsString>,
}

impl FfmpegArgs {
    pub fn new() -> Self {
        Self { args: Vec::new() }
    }

    /// Append one bare flag/value (e.g. `"-y"`, `"error"`).
    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_os_string());
        self
    }

    /// Append several bare flags/values in order.
    pub fn args<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in items {
            self.args.push(a.as_ref().to_os_string());
        }
        self
    }

    /// `-i <path>` — an input file. `path` is passed as a single argument,
    /// never interpolated into a string.
    pub fn input(self, path: &Path) -> Self {
        self.arg("-i").arg(path.as_os_str())
    }

    /// A bare positional path argument (ffmpeg's output path, or ffprobe's
    /// input path when not preceded by `-i`).
    pub fn path(self, path: &Path) -> Self {
        self.arg(path.as_os_str())
    }

    pub fn build(self) -> Vec<OsString> {
        self.args
    }

    pub fn as_slice(&self) -> &[OsString] {
        &self.args
    }
}

/// Run `binary` with `args` to completion, capturing stdout/stderr. Used for
/// short-lived calls (ffprobe, single-frame thumbnail extraction) where
/// there's no meaningful incremental progress to report.
pub fn run_capture(binary: &Path, args: &FfmpegArgs) -> Result<Output> {
    Command::new(binary)
        .args(args.as_slice())
        .output()
        .with_context(|| format!("spawning {}", binary.display()))
}

/// Run `binary` with `args`, requiring a zero exit status. Returns stderr
/// (trimmed) as the error message on failure — ffmpeg/ffprobe put their
/// diagnostics there, never stdout.
pub fn run_checked(binary: &Path, args: &FfmpegArgs) -> Result<Output> {
    let out = run_capture(binary, args)?;
    if !out.status.success() {
        return Err(anyhow!(
            "{} exited with {}: {}",
            binary.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out)
}

/// One `-progress pipe:1` update. ffmpeg emits a block of `key=value` lines
/// terminated by a `progress=continue`/`progress=end` line; `parse_progress`
/// below turns each block into one of these.
#[derive(Debug, Clone, Default)]
pub struct FfmpegProgress {
    pub out_time_us: Option<i64>,
    pub speed: Option<f64>,
    pub done: bool,
}

/// Parse one `-progress` block's raw `key=value\n` lines (as accumulated by
/// the caller between two `progress=` lines) into an `FfmpegProgress`.
/// Split out from the process-reading loop so it can be unit tested against
/// fixed strings without spawning ffmpeg.
pub fn parse_progress_block(block: &str) -> FfmpegProgress {
    let mut progress = FfmpegProgress::default();
    for line in block.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "out_time_us" | "out_time_ms" => {
                // ffmpeg has used both key names across versions for the same
                // microsecond count (the `_ms` name is a historical
                // misnomer, not a unit change) — accept either.
                progress.out_time_us = value.parse::<i64>().ok();
            }
            "speed" => {
                // e.g. "2.5x" — strip the trailing 'x'.
                progress.speed = value.trim_end_matches('x').trim().parse::<f64>().ok();
            }
            "progress" => {
                progress.done = value == "end";
            }
            _ => {}
        }
    }
    progress
}

/// Spawn `binary` with `args` (which must already include
/// `-progress pipe:1 -nostats`), streaming stdout progress blocks to
/// `on_progress` as they arrive, and return the final exit status. Used by
/// proxy generation to drive Tauri progress events.
///
/// Cancellation kills the child and returns `Err` immediately; the caller is
/// responsible for deleting any partial output file (mirrors autocut's
/// `audio.rs`/`export_mp4.rs` cancellation convention — audit §2).
pub fn run_with_progress(
    binary: &Path,
    args: &FfmpegArgs,
    mut on_progress: impl FnMut(FfmpegProgress),
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<std::process::ExitStatus> {
    use std::io::{BufRead, BufReader};
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    let mut child = Command::new(binary)
        .args(args.as_slice())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {}", binary.display()))?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let mut stderr_pipe = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        buf
    });

    // Read stdout line-by-line on a background thread (blocking I/O), and
    // forward each accumulated `key=value...progress=...` block to the
    // caller's thread via a channel — this is what lets the calling thread
    // poll `cancel` and kill the child even while the reader thread is
    // blocked on a `read_line` call.
    let (tx, rx) = mpsc::channel::<FfmpegProgress>();
    let stdout_reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut block = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let is_marker = line.trim_start().starts_with("progress=");
                    block.push_str(&line);
                    if is_marker {
                        let progress = parse_progress_block(&block);
                        block.clear();
                        if tx.send(progress).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(anyhow!("ffmpeg operation cancelled"));
            }
        }
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(progress) => on_progress(progress),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(status) = child.try_wait().context("polling ffmpeg status")? {
                    let _ = stdout_reader.join();
                    let stderr = stderr_reader.join().unwrap_or_default();
                    if !status.success() {
                        return Err(anyhow!(
                            "{} exited with {}: {}",
                            binary.display(),
                            status,
                            stderr.trim()
                        ));
                    }
                    return Ok(status);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.wait().context("waiting for ffmpeg")?;
                let stderr = stderr_reader.join().unwrap_or_default();
                if !status.success() {
                    return Err(anyhow!(
                        "{} exited with {}: {}",
                        binary.display(),
                        status,
                        stderr.trim()
                    ));
                }
                return Ok(status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builds_an_argument_array_never_a_joined_string() {
        let path = PathBuf::from("D:/My Videos/Test Video.mp4");
        let args = FfmpegArgs::new()
            .arg("-y")
            .input(&path)
            .args(["-vn", "-ac", "1"])
            .build();

        // The space-containing path must survive as exactly one argument,
        // not "-i" + "D:/My" + "Videos/Test" + "Video.mp4" the way naive
        // string-splitting on whitespace would produce.
        assert_eq!(
            args,
            vec![
                OsString::from("-y"),
                OsString::from("-i"),
                OsString::from("D:/My Videos/Test Video.mp4"),
                OsString::from("-vn"),
                OsString::from("-ac"),
                OsString::from("1"),
            ]
        );
    }

    #[test]
    fn preserves_unicode_and_vietnamese_filenames_as_single_arguments() {
        let path = PathBuf::from("C:/Video tiếng Việt/phỏng vấn 01.mp4");
        let args = FfmpegArgs::new().input(&path).build();
        assert_eq!(args.len(), 2);
        assert_eq!(
            args[1].to_string_lossy(),
            "C:/Video tiếng Việt/phỏng vấn 01.mp4"
        );
    }

    #[test]
    fn parses_a_progress_block_with_out_time_and_speed() {
        let block = "frame=120\nfps=30.0\nout_time_us=4000000\nspeed=2.1x\nprogress=continue\n";
        let progress = parse_progress_block(block);
        assert_eq!(progress.out_time_us, Some(4_000_000));
        assert_eq!(progress.speed, Some(2.1));
        assert!(!progress.done);
    }

    #[test]
    fn parses_the_terminal_progress_end_marker() {
        let progress = parse_progress_block("out_time_us=9999999\nprogress=end\n");
        assert!(progress.done);
    }

    #[test]
    fn ignores_unrecognized_keys_without_erroring() {
        let progress =
            parse_progress_block("bitrate=128kbits/s\ndup_frames=0\nprogress=continue\n");
        assert_eq!(progress.out_time_us, None);
        assert!(!progress.done);
    }

    #[test]
    fn run_checked_surfaces_stderr_on_failure() {
        // A made-up flag ffmpeg/ffprobe binaries reject universally — this
        // exercises the real subprocess path against the actual ffmpeg
        // resolved on the remote test server (dev-mode PATH fallback), not a
        // mock.
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let args = FfmpegArgs::new().arg("-not-a-real-flag");
        let result = run_checked(&ffmpeg, &args);
        assert!(result.is_err());
    }
}
