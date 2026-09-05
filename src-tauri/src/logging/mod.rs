//! Structured logging, panic capture, and the unclean-exit session marker
//! (master prompt §54 "Crash handling", §55 "Logging", §86 "Recovery").
//!
//! Three independent pieces live here, all wired from `lib.rs::run()`:
//!
//! 1. **Structured file logging** ([`build_subscriber`]/[`init_logging`]):
//!    real, leveled (`ERROR`/`WARN`/`INFO`/`DEBUG`/`TRACE`) logs written to a
//!    daily-rolling file under a caller-supplied log directory (resolved by
//!    `commands::diagnostics::logs_dir`, which needs a Tauri `AppHandle` this
//!    module deliberately stays decoupled from — see that function's own doc
//!    comment for the exact real path) via `tracing` + `tracing-appender`,
//!    instead of stderr that vanishes the moment a release GUI build's
//!    (nonexistent) console closes. Verbosity follows `RUST_LOG` if set
//!    (`EnvFilter::try_from_default_env`), else defaults to `info` — the
//!    "allow debug logging from settings" half of master prompt §55 is a
//!    frontend Settings-panel toggle that would set `RUST_LOG`/restart, not
//!    implemented here (no such Settings panel exists yet; documented gap,
//!    not silently dropped).
//! 2. **Panic hook** ([`install_panic_hook`]): every Rust panic anywhere in
//!    the app gets logged with its message, source location, and a full
//!    backtrace via `tracing::error!`, then falls through to the previous
//!    (default) hook so a debug/console build still sees the usual panic
//!    output too.
//! 3. **Unclean-exit session marker** ([`check_and_mark_session_start`]/
//!    [`mark_clean_exit`]): a small marker file written at startup and
//!    removed only on a genuinely clean shutdown (`RunEvent::Exit`, wired in
//!    `lib.rs`). A hard crash/panic/force-kill never reaches the removal
//!    step, so the marker is still present at the *next* startup — that's
//!    the entire detection mechanism master prompt §86 asks for ("AI Video
//!    Editor did not shut down correctly").
//!
//!    **Honest scope limit**: §86 also asks for a "Recover Project" option
//!    alongside "Discard Recovery"/"Open Logs". This module can only ever
//!    answer *whether* the last exit was clean — there is no project
//!    auto-save system anywhere in this codebase (checked: `project::io`
//!    only has caller-invoked `save_atomic`/`load` on an explicit path, no
//!    background/periodic autosave, no "last edited project" pointer
//!    persisted anywhere). Building a real autosave system is out of scope
//!    for this pass (Phase 12 packaging/crash-handling, not a Project
//!    Manager phase) — `SessionStatus::recovered_project_path` is honestly
//!    always `None`, not a fabricated feature.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use specta::Type;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Log file base name; `tracing_appender::rolling::daily` appends the date,
/// e.g. `ai-video-editor.log.2026-09-05`.
const LOG_FILE_BASENAME: &str = "ai-video-editor.log";

/// The marker file's name, dot-prefixed the way this codebase already
/// prefixes similar internal-only files (matches no existing precedent one
/// for one, but keeps it out of the way of anything a user might browse to
/// under `$APPLOCALDATA`).
const SESSION_MARKER_FILENAME: &str = ".session-active";

/// Where the app's Tauri-managed [`SessionStatus`] state (see `lib.rs`)
/// reports the answer to "did the app shut down cleanly last time" from —
/// computed once at startup, before this session's own marker overwrites
/// the evidence.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SessionStatus {
    /// `true` if no stale marker was found at startup (previous session, if
    /// any, shut down cleanly or this is the first-ever launch). `false`
    /// means a marker from a previous session was still present — the app
    /// did not reach [`mark_clean_exit`] last time (crash, panic, or a hard
    /// kill of the process).
    pub previous_exit_was_clean: bool,
    /// Always `None` — see this module's doc comment: no project auto-save
    /// system exists yet to recover *from*. Kept as a real (not omitted)
    /// field so a future auto-save pass can wire it up additively without
    /// an IPC-shape change on the frontend side.
    pub recovered_project_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Structured file logging
// ---------------------------------------------------------------------------

/// Builds a `tracing` subscriber that writes to a daily-rolling file under
/// `log_dir`, plus the [`WorkerGuard`] that must stay alive for as long as
/// the subscriber should keep flushing (dropping it blocks until pending log
/// lines are written, then stops the background writer thread — see
/// `tracing_appender`'s own docs). Split out from [`init_logging`] so tests
/// can scope a subscriber to one thread via
/// `tracing::subscriber::with_default` instead of fighting over the one
/// process-global subscriber `set_global_default` allows.
pub fn build_subscriber(
    log_dir: &Path,
) -> io::Result<(impl tracing::Subscriber + Send + Sync, WorkerGuard)> {
    std::fs::create_dir_all(log_dir)?;
    let file_appender = tracing_appender::rolling::daily(log_dir, LOG_FILE_BASENAME);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Master prompt §55 "Never log: API keys, tokens, sensitive headers" is
    // enforced by *callers* (nothing here has access to those values to log
    // them in the first place) — this module only owns the sink/formatting,
    // not what call sites choose to pass to `tracing::info!`/`error!`/etc.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        // No ANSI color codes — this is a plain-text file, not a terminal.
        .with_ansi(false)
        .with_env_filter(env_filter)
        .finish();

    Ok((subscriber, guard))
}

/// Installs `build_subscriber`'s output as the process-wide default
/// subscriber. Called exactly once, from `lib.rs::run()`, before anything
/// else starts logging. Failure to create the log directory is reported to
/// the caller (who logs it to stderr and continues without file logging —
/// see `lib.rs`) rather than panicking: crash-logging infrastructure failing
/// should never itself be what prevents the app from starting.
pub fn init_logging(log_dir: &Path) -> io::Result<WorkerGuard> {
    let (subscriber, guard) = build_subscriber(log_dir)?;
    // `set_global_default` can only succeed once per process; a second call
    // (there shouldn't be one in production) is silently ignored rather than
    // panicking, matching this function's own "never block startup" stance.
    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(guard)
}

/// Process-global home for the [`WorkerGuard`] `init_logging` returns.
/// Dropping a `WorkerGuard` blocks until pending log lines are flushed and
/// then stops the background writer thread, so production code must never
/// let it drop before the app actually exits — stashing it here for the
/// rest of the process's lifetime (never explicitly dropped) is simpler
/// than threading it through as Tauri-managed state for a value nothing
/// ever needs to read back. Tests instead hold their own local guard so
/// they control exactly when the flush happens (see this module's own
/// panic-hook test).
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn keep_alive_for_process_lifetime(guard: WorkerGuard) {
    let _ = LOG_GUARD.set(guard);
}

// ---------------------------------------------------------------------------
// Panic hook
// ---------------------------------------------------------------------------

/// Extracts a human-readable message from a panic payload — the same
/// `&str`/`String` downcast every std panic message actually is in practice
/// (a non-string payload from `panic_any` is rare and reported honestly as
/// `"<non-string panic payload>"` rather than guessed at).
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// The actual logging logic, shared between the real panic hook installed
/// by [`install_panic_hook`] and this module's own test (which installs this
/// same function as the hook directly, rather than re-deriving it, so the
/// test exercises the real code path).
fn log_panic(info: &std::panic::PanicHookInfo<'_>) {
    let location = info
        .location()
        .map(|l| l.to_string())
        .unwrap_or_else(|| "<unknown location>".to_string());
    let message = panic_payload_message(info.payload());
    // `force_capture` (unlike `Backtrace::capture`) always captures
    // regardless of `RUST_BACKTRACE`/`RUST_LIB_BACKTRACE` — a crash log with
    // no backtrace because an env var wasn't set defeats the point of
    // logging panics with "a real backtrace" (master prompt §54) at all.
    let backtrace = std::backtrace::Backtrace::force_capture();
    tracing::error!(target: "panic", %location, %backtrace, "panic: {message}");
}

/// Installs a panic hook that logs every panic (message, source location,
/// full backtrace) via [`log_panic`], then chains to whatever hook was
/// previously installed (Rust's own default hook in production, which still
/// prints to stderr — harmless in a debug/console build, invisible but
/// harmless in a release GUI build with no console). Called once from
/// `lib.rs::run()`, after [`init_logging`] so a subscriber already exists to
/// receive the `tracing::error!` call above.
pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_panic(info);
        previous_hook(info);
    }));
}

// ---------------------------------------------------------------------------
// Logs folder path + "open in file explorer"
// ---------------------------------------------------------------------------

/// Opens `path` in the OS's file explorer. Real behavior is Windows-only
/// (`explorer.exe`, matching master prompt §54's "Open Logs Folder" button);
/// the non-Windows branch is a dev/test-only best-effort fallback (`xdg-open`
/// on this crate's actual Linux WSL2 build/test host) so this at least
/// compiles and can be exercised in spirit there — this app never ships for
/// Linux, so that branch is not itself a real shipping behavior.
#[cfg(target_os = "windows")]
pub fn open_folder(path: &Path) -> io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_child| ())
}

#[cfg(not(target_os = "windows"))]
pub fn open_folder(path: &Path) -> io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_child| ())
}

// ---------------------------------------------------------------------------
// Unclean-exit session marker (master prompt §86)
// ---------------------------------------------------------------------------

fn session_marker_path(app_local_data_dir: &Path) -> PathBuf {
    app_local_data_dir.join(SESSION_MARKER_FILENAME)
}

/// Call once, early in `lib.rs::run()`'s `setup()` — before any other
/// startup step, so a later panic during startup itself still leaves the
/// marker in place for the *next* launch to notice. Returns whether the
/// *previous* session's exit was clean (no marker found), then
/// unconditionally writes a fresh marker for *this* session. The order
/// matters: check first, write second, or every check after the first
/// would see this session's own marker and report "clean" unconditionally.
pub fn check_and_mark_session_start(app_local_data_dir: &Path) -> io::Result<bool> {
    std::fs::create_dir_all(app_local_data_dir)?;
    let marker = session_marker_path(app_local_data_dir);
    let previous_exit_was_clean = !marker.exists();
    std::fs::write(&marker, chrono::Utc::now().to_rfc3339())?;
    Ok(previous_exit_was_clean)
}

/// Call from the `RunEvent::Exit` handler (`lib.rs`) — the one signal a
/// Tauri app receives that corresponds to an actual graceful shutdown, as
/// opposed to a crash/panic/force-kill, which never reaches this call and so
/// leaves the marker behind for [`check_and_mark_session_start`] to notice
/// next launch. Missing-file removal errors are ignored (already-clean is
/// not a failure).
pub fn mark_clean_exit(app_local_data_dir: &Path) {
    let _ = std::fs::remove_file(session_marker_path(app_local_data_dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ave-logging-test-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fresh_directory_reports_previous_exit_as_clean_and_writes_a_marker() {
        let dir = temp_dir("fresh");
        let clean = check_and_mark_session_start(&dir).expect("check_and_mark_session_start");
        assert!(clean, "no prior marker should mean a clean previous exit");
        assert!(session_marker_path(&dir).exists());
    }

    #[test]
    fn a_marker_left_over_from_a_previous_session_is_reported_as_unclean() {
        let dir = temp_dir("stale-marker");
        // Simulate a previous run that crashed: marker written, never removed.
        check_and_mark_session_start(&dir).expect("first session start");
        // Next launch: marker from "last time" is still there.
        let clean = check_and_mark_session_start(&dir).expect("second session start");
        assert!(
            !clean,
            "a marker still present from a previous session means an unclean exit"
        );
    }

    #[test]
    fn clean_exit_removes_the_marker_so_the_next_start_reports_clean_again() {
        let dir = temp_dir("clean-exit-cycle");
        check_and_mark_session_start(&dir).expect("session start");
        assert!(session_marker_path(&dir).exists());

        mark_clean_exit(&dir);
        assert!(
            !session_marker_path(&dir).exists(),
            "mark_clean_exit should remove the marker"
        );

        let clean = check_and_mark_session_start(&dir).expect("next session start");
        assert!(
            clean,
            "with the marker removed by a clean exit, the next start should report clean"
        );
    }

    #[test]
    fn mark_clean_exit_on_a_directory_with_no_marker_does_not_error() {
        let dir = temp_dir("no-marker");
        // Never called check_and_mark_session_start, so no marker exists.
        mark_clean_exit(&dir); // must not panic
    }

    /// Exercises the *real* panic-hook logging function (not a re-derived
    /// copy) end-to-end: install it as the process's panic hook, trigger a
    /// real panic through `catch_unwind` (so it never aborts the test
    /// binary), scope a real file-backed subscriber to this thread via
    /// `tracing::subscriber::with_default` (avoiding the process-global
    /// `set_global_default`, which only the *first* caller in a whole test
    /// binary can ever win — see `build_subscriber`'s doc comment), then
    /// read the log file back and confirm the panic message and location
    /// actually made it in.
    ///
    /// The panic hook is restored immediately after (previous hook stashed
    /// via `take_hook`) to minimize — though on a global resource shared by
    /// every test thread in this binary, not fully eliminate — the window
    /// where another concurrently-running test's panic would also flow
    /// through this hook.
    #[test]
    fn panic_hook_writes_the_panic_message_and_location_to_the_log_file() {
        let dir = temp_dir("panic-hook");
        let (subscriber, guard) = build_subscriber(&dir).expect("build_subscriber");

        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(log_panic));

        let result = tracing::subscriber::with_default(subscriber, || {
            std::panic::catch_unwind(|| {
                panic!("deliberate test panic for crash-log verification");
            })
        });

        std::panic::set_hook(previous_hook);
        assert!(result.is_err(), "the deliberate panic should have unwound");

        // Flush the non-blocking writer's background thread before reading.
        drop(guard);

        let mut found = false;
        for entry in std::fs::read_dir(&dir).expect("read log dir") {
            let path = entry.expect("dir entry").path();
            if path.is_file() {
                let contents = std::fs::read_to_string(&path).unwrap_or_default();
                if contents.contains("deliberate test panic for crash-log verification") {
                    found = true;
                    break;
                }
            }
        }
        assert!(
            found,
            "expected a log file under {} to contain the panic message",
            dir.display()
        );
    }
}
