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

use std::collections::HashSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};

// ---------------------------------------------------------------------------
// Last-resort child-process registry (master prompt §45: never leave
// ffmpeg.exe/sidecar.exe running after application exit).
//
// Cooperative cancellation (the `AtomicBool` flag every long-running caller
// here already threads through) only kills the real OS child if the worker
// thread that's polling that flag gets scheduled again before the whole
// process exits — true on a user-initiated Cancel click (plenty of time),
// NOT guaranteed true when the *application itself* is closing: Tauri/winit's
// event loop calls `RunEvent::Exit`'s handler once and then, on several
// platforms, terminates the process shortly after that closure returns
// rather than waiting for every background thread to notice a flag change.
// A still-running ffmpeg child at that moment would otherwise survive as a
// genuine orphan (Rust's `Child` does not kill its process on `Drop`, and
// Windows does not tie a child process's lifetime to its parent's unless
// explicitly configured to via a Job Object, which this codebase does not
// set up).
//
// This registry tracks every currently-spawned ffmpeg/ffprobe-family child's
// OS pid (`TrackedChildPid`, an RAII guard constructed right after a
// successful `spawn()` and dropped — untracking itself — on every return
// path once that child has been waited on). `kill_all_tracked_children`,
// wired into `lib.rs`'s `RunEvent::Exit` handler, force-kills anything still
// registered at that point as a final safety net on top of (not instead of)
// the existing cooperative-cancellation paths.
fn registry() -> &'static Mutex<HashSet<u32>> {
    static REGISTRY: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashSet::new()))
}

/// RAII guard: tracks `pid` in the process-wide registry for as long as it's
/// held, untracking automatically on `Drop` (so every return path — success,
/// error, or cancellation — cleans up without needing to remember to do so
/// explicitly). Construct immediately after a successful `Command::spawn()`.
pub(crate) struct TrackedChildPid(u32);

impl TrackedChildPid {
    pub(crate) fn new(pid: u32) -> Self {
        registry()
            .lock()
            .expect("child pid registry poisoned")
            .insert(pid);
        Self(pid)
    }
}

impl Drop for TrackedChildPid {
    fn drop(&mut self) {
        registry()
            .lock()
            .expect("child pid registry poisoned")
            .remove(&self.0);
    }
}

/// Force-kills every still-tracked child pid — a last-resort application-exit
/// sweep (module doc comment above), not the normal cancellation path (normal
/// per-job cancellation already calls `Child::kill()` directly on its own
/// child and relies on `TrackedChildPid`'s `Drop` to untrack it). Returns how
/// many pids it attempted to kill, for diagnostics/tests. Best-effort: a pid
/// that has already exited on its own simply fails to kill, which is not
/// treated as an error (there is nothing left to clean up).
pub fn kill_all_tracked_children() -> usize {
    let pids: Vec<u32> = registry()
        .lock()
        .expect("child pid registry poisoned")
        .iter()
        .copied()
        .collect();
    for pid in &pids {
        kill_pid_forcefully(*pid);
    }
    pids.len()
}

#[cfg(target_os = "windows")]
fn kill_pid_forcefully(pid: u32) {
    // `/T` also kills any child of this pid (ffmpeg does not normally spawn
    // its own children, but this is a last-resort sweep — no reason not to
    // be thorough). Best-effort: ignore the exit status/output entirely, a
    // pid that's already gone is not a failure of this cleanup pass.
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

#[cfg(not(target_os = "windows"))]
fn kill_pid_forcefully(pid: u32) {
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
}

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
    // Registered for the lifetime of this function, on every return path
    // (module-level doc comment: app-exit orphan safety net, §45).
    let _tracked = TrackedChildPid::new(child.id());

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

    // -- §88 Windows path edge cases: other non-ASCII Unicode, a very long
    //    path, and a UNC-shaped path string, on top of the Vietnamese/spaces
    //    cases already above -------------------------------------------------

    #[test]
    fn preserves_other_non_ascii_unicode_japanese_and_emoji_as_a_single_argument() {
        let path = PathBuf::from("C:/動画プロジェクト/最終版 🎬.mp4");
        let args = FfmpegArgs::new().input(&path).build();
        assert_eq!(args.len(), 2);
        assert_eq!(
            args[1].to_string_lossy(),
            "C:/動画プロジェクト/最終版 🎬.mp4"
        );
    }

    #[test]
    fn preserves_a_very_long_path_as_a_single_argument() {
        // Windows' classic MAX_PATH is 260 characters — this proves
        // `FfmpegArgs` itself never truncates/splits a long `OsString`
        // argument (it's just a `Vec<OsString>`, no length cap anywhere in
        // this builder). Windows' own enforcement of that limit (and
        // whether a `\\?\` prefix would be needed) is not something this
        // Linux/WSL2 test environment can verify — see `HANDOFF.md`.
        let long_dir = "a-long-directory-name-repeated-".repeat(10);
        let path_string = format!("D:/{long_dir}/video.mp4");
        assert!(path_string.len() > 260, "{}", path_string.len());
        let path = PathBuf::from(&path_string);

        let args = FfmpegArgs::new().input(&path).build();
        assert_eq!(args.len(), 2);
        assert_eq!(args[1].to_string_lossy(), path_string);
    }

    #[test]
    fn preserves_a_unc_shaped_path_as_a_single_argument_without_splitting_on_backslash() {
        // UNC paths (`\\server\share\...`) are a Windows-specific string
        // convention; real UNC network access can only be verified on real
        // Windows. This proves `FfmpegArgs` carries a UNC-shaped string as
        // exactly one argument regardless — it never assumes forward-
        // slash-only splitting (it doesn't split at all; `.input()` hands
        // the whole `OsStr` through untouched), so this is real coverage of
        // this builder's own string handling, not of real UNC filesystem
        // access.
        let path = PathBuf::from(r"\\server\share\Video Projects\clip.mp4");
        let args = FfmpegArgs::new().input(&path).build();
        assert_eq!(args.len(), 2);
        assert_eq!(
            args[1].to_string_lossy(),
            r"\\server\share\Video Projects\clip.mp4"
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

    // -- §45 orphan-process verification --------------------------------
    //
    // These confirm the actual OS-level process is gone after cancellation
    // or an app-exit sweep, not merely that the Rust-level call returned
    // `Err`/a count. `/proc/<pid>` existence is a Linux-specific check (this
    // crate's real dev/test environment is WSL2 — see `HANDOFF.md`); the
    // equivalent real-Windows verification (confirming no orphaned
    // `ffmpeg.exe` survives in Task Manager / `Get-Process`) still needs a
    // manual pass on an actual Windows machine, since WSL2 cannot observe
    // Windows process-tree semantics directly.

    #[cfg(not(target_os = "windows"))]
    fn process_exists(pid: u32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(not(target_os = "windows"))]
    fn find_process_with_marker(marker: &str) -> Option<u32> {
        let out = Command::new("ps").args(["-eo", "pid,args"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains(marker) {
                let pid_str = line.split_whitespace().next()?;
                if let Ok(pid) = pid_str.parse::<u32>() {
                    return Some(pid);
                }
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    fn wait_until(mut condition: impl FnMut() -> bool, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if condition() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        condition()
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn cancelling_mid_flight_actually_kills_the_real_os_process_not_just_the_rust_future() {
        // The specific thing §45 asks to be proven, not assumed: cancel a
        // *real*, deliberately-long-running ffmpeg process mid-encode and
        // confirm the OS process is genuinely gone afterward — found via
        // `ps`/`/proc`, independent of `run_with_progress`'s own return
        // value.
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let dir =
            std::env::temp_dir().join(format!("ave-ffmpeg-orphan-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.mp4");
        let marker = out.to_string_lossy().into_owned();

        // `-re` throttles lavfi input reading to real (wall-clock) time, so
        // this genuinely runs for ~8 seconds regardless of how fast this
        // machine can encode 320x240 — a deterministic "still running"
        // window to cancel into, unlike guessing at raw encode speed.
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=8:size=320x240:rate=25",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-progress",
                "pipe:1",
                "-nostats",
            ])
            .path(&out);

        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_for_thread = cancel.clone();
        let ffmpeg_owned = ffmpeg.clone();
        let join_handle = std::thread::spawn(move || {
            run_with_progress(
                &ffmpeg_owned,
                &args,
                |_| {},
                Some(cancel_for_thread.as_ref()),
            )
        });

        let pid = wait_until(
            || find_process_with_marker(&marker).is_some(),
            std::time::Duration::from_secs(5),
        )
        .then(|| find_process_with_marker(&marker))
        .flatten()
        .expect("expected to observe the real spawned ffmpeg process via `ps`");

        // Give it a genuine moment mid-encode (not just spawned) before
        // cancelling.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            process_exists(pid),
            "ffmpeg should still be running before cancel"
        );

        cancel.store(true, std::sync::atomic::Ordering::SeqCst);

        let result = join_handle
            .join()
            .expect("run_with_progress thread should not panic");
        assert!(result.is_err(), "a cancelled run must return Err");

        // The real, OS-level assertion: the process is actually gone, not
        // merely abandoned by the Rust side.
        assert!(
            wait_until(|| !process_exists(pid), std::time::Duration::from_secs(3)),
            "ffmpeg process {pid} should be killed on cancel, not orphaned"
        );
        // Checked by specific pid, not a global `tracked_child_count()` ==
        // 0 — `cargo test` runs this whole crate's tests in parallel in one
        // process, and other tests (elsewhere in this file, and in
        // `audio::pcm`) may have their own, entirely unrelated real
        // children registered in this same process-wide registry at this
        // exact moment. Asserting the global count would be a spurious,
        // flaky failure; asserting *our* pid specifically is untracked is
        // the real, correct thing this test is proving.
        assert!(
            !registry().lock().unwrap().contains(&pid),
            "our pid must be untracked after cancellation"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tracked_child_pid_inserts_on_construction_and_removes_itself_on_drop() {
        // The RAII contract `run_with_progress`/`audio::pcm` both rely on,
        // tested in isolation against a synthetic (non-real-process) pid
        // number so it can never collide with — or affect — any real
        // process another parallel test may have spawned. Deliberately not
        // a real subprocess: this test is about the bookkeeping, not the
        // killing (see `kill_pid_forcefully_actually_terminates_a_real_process`
        // for that).
        let fake_pid: u32 = 4_000_000_001;
        {
            let _tracked = TrackedChildPid::new(fake_pid);
            assert!(registry().lock().unwrap().contains(&fake_pid));
        }
        assert!(!registry().lock().unwrap().contains(&fake_pid));
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn kill_pid_forcefully_actually_terminates_a_real_process() {
        // The real kill primitive `kill_all_tracked_children`'s app-exit
        // sweep calls on every still-tracked pid (module doc comment) —
        // tested directly against a dedicated real process this test alone
        // owns, deliberately *not* via the whole-registry
        // `kill_all_tracked_children()` sweep itself: that function iterates
        // every pid currently in the process-wide registry, and since
        // `cargo test` runs this crate's tests in parallel in one process,
        // calling the real sweep here could kill another concurrently-
        // running test's legitimate ffmpeg child out from under it. Testing
        // the per-pid primitive it's built from proves the same thing (a
        // real OS process is genuinely terminated) without that hazard.
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a real long-lived process");
        let pid = child.id();
        assert!(process_exists(pid), "sanity: process is really running");

        kill_pid_forcefully(pid);

        // Reap it so it doesn't linger as a zombie; kill -9 + wait is
        // near-instant.
        let _ = child.wait();
        assert!(
            wait_until(|| !process_exists(pid), std::time::Duration::from_secs(3)),
            "the process must actually be gone after a forceful kill"
        );
    }
}
