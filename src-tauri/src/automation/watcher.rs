//! `AppHandle`-free "new video arrived in this folder" primitive
//! (`AutomationTrigger::WatchFolder`, upgrade spec §27): wraps the real
//! `notify` crate filesystem watch, filters to file-*create* events for a
//! known video extension (reusing `crate::media::import::classify_extension`
//! — never a second extension list), and debounces a still-being-written/
//! copied file by polling its size for real stability before ever calling
//! back. No `AppHandle` anywhere in this file — directly unit-testable with
//! a real temp directory and a real file write, mirroring every other "pure
//! core, thin `AppHandle`-dependent wrapper" split in this codebase
//! (`batch::manager::create_batch` vs. `start_batch`, etc.);
//! `automation::manager::RuleWatcherManager` is the thin, `AppHandle`-aware
//! layer built on top of this one that supplies the real callback (probe ->
//! condition -> run pipeline -> record).
//!
//! ## Why `notify` (and not GUI/RPA automation of any kind)
//!
//! `notify` is the standard, actively-maintained cross-platform Rust
//! file-watching crate (inotify on Linux, `ReadDirectoryChangesW` on
//! Windows — this app's actual target) — chosen because nothing else in
//! this codebase watches the filesystem today, and this is a genuinely new
//! capability, not a reimplementation of an existing one. This has nothing
//! to do with CapCut automation (out of scope per `UPGRADE_PLAN.md`'s own
//! "Explicitly out of scope" section) — it only watches a plain folder on
//! disk for new files; the *action* a rule takes once a file arrives is the
//! existing batch pipeline (`automation` module doc comment).
//!
//! ## Debounce design: manual size-stability polling, not `notify-debouncer-*`
//!
//! A "new video added to folder X" trigger must not fire while the OS is
//! still copying/writing that file — a `Create` event fires the instant the
//! file is *created* (often at 0 bytes), well before a large video finishes
//! copying. Two real options existed: pull in
//! `notify-debouncer-full`/`notify-debouncer-mini` (built for exactly this),
//! or implement a small manual poll. This module deliberately picks the
//! manual poll: those debouncer crates coalesce/delay *event delivery*
//! ("wait N ms after the last event for this path before delivering it"),
//! the right tool for a noisy stream of modify/rename events — but they
//! don't know whether a file's *content* has actually finished changing. A
//! debounced `Create` event could still fire while a multi-GB copy is only
//! 10% done. What this trigger actually needs is "wait until the file's
//! size stops changing", which is simpler to poll directly
//! ([`wait_until_stable`]) than to configure a duration-based coalescing
//! library to approximate — and it avoids a second crate plus pinning its
//! own compatible `notify` version.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::media::import::classify_extension;
use crate::project::MediaKind;

/// How often to re-check a candidate file's size while waiting for it to
/// stop changing.
const STABILITY_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Consecutive unchanged-size polls required before a file is considered
/// "done arriving" — `5 * 200ms` = 1s of real stability, inside this
/// trigger's own documented "wait briefly, ~1-2 seconds" target.
const STABILITY_CHECKS_REQUIRED: u32 = 5;
/// Give up waiting (and skip the file) after this long — protects against a
/// file that never stops growing (e.g. an active recording being written
/// straight into the watched folder) from blocking this thread forever.
const STABILITY_MAX_WAIT: Duration = Duration::from_secs(30);

/// Polls `path`'s size on disk until it holds steady for
/// `STABILITY_CHECKS_REQUIRED` consecutive polls, or gives up after
/// `STABILITY_MAX_WAIT`. Returns `false` if the file disappeared/became
/// unreadable while waiting (e.g. a temp file that got renamed away) or if
/// stability was never reached in time — either way, the caller must not
/// treat the file as "arrived".
pub(crate) fn wait_until_stable(path: &Path) -> bool {
    let start = Instant::now();
    let mut last_size: Option<u64> = None;
    let mut stable_count = 0u32;
    loop {
        match std::fs::metadata(path) {
            Ok(meta) => {
                let size = meta.len();
                if last_size == Some(size) {
                    stable_count += 1;
                    if stable_count >= STABILITY_CHECKS_REQUIRED {
                        return true;
                    }
                } else {
                    last_size = Some(size);
                    stable_count = 0;
                }
            }
            Err(_) => return false,
        }
        if start.elapsed() >= STABILITY_MAX_WAIT {
            return false;
        }
        std::thread::sleep(STABILITY_POLL_INTERVAL);
    }
}

/// Starts a real `notify` watch on `folder` (non-recursive — a "new video
/// added to folder X" trigger, not a whole subtree), calling `on_arrived`
/// exactly once per file that (a) was just *created*, (b) has a known video
/// extension (`classify_extension`, reused from `media::import` — never a
/// second list; matches this trigger's own "new **video**" wording, upgrade
/// spec §27), and (c) subsequently held a stable size for real
/// ([`wait_until_stable`], on its own spawned thread so one slow-arriving
/// file never blocks this watcher from noticing the next event).
///
/// The returned `RecommendedWatcher` must be kept alive by the caller for as
/// long as the watch should remain active — dropping it stops the
/// underlying OS watch (`notify`'s own `Drop` behavior, not custom logic
/// here).
pub(crate) fn watch_folder(
    folder: &Path,
    on_arrived: impl Fn(PathBuf) + Send + Sync + 'static,
) -> notify::Result<RecommendedWatcher> {
    let on_arrived = Arc::new(on_arrived);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else {
            return;
        };
        if !matches!(event.kind, EventKind::Create(_)) {
            return;
        }
        for path in event.paths {
            if classify_extension(&path) != Some(MediaKind::Video) {
                continue;
            }
            let cb = on_arrived.clone();
            std::thread::spawn(move || {
                if wait_until_stable(&path) {
                    cb(path);
                }
            });
        }
    })?;
    watcher.watch(folder, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-automation-watcher-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn wait_until_stable_returns_true_once_a_files_size_stops_changing() {
        let dir = temp_dir("stable");
        let path = dir.join("clip.mp4");
        fs::write(&path, b"final content, never touched again").unwrap();
        assert!(wait_until_stable(&path));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wait_until_stable_returns_false_for_a_path_that_never_existed() {
        let dir = temp_dir("missing");
        let path = dir.join("does-not-exist.mp4");
        assert!(!wait_until_stable(&path));
        fs::remove_dir_all(&dir).ok();
    }

    /// Real end-to-end: a real `notify` watch on a real temp folder, a real
    /// file genuinely created into it during this test (not a simulated
    /// event) — asserts the watcher really fires.
    #[test]
    fn watch_folder_fires_on_a_real_new_video_file_created_into_the_folder() {
        let dir = temp_dir("real-watch");
        let seen: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_for_cb = seen.clone();
        let _watcher = watch_folder(&dir, move |path| {
            seen_for_cb.lock().unwrap().push(path);
        })
        .expect("starting a real watch on a real temp dir");

        let video_path = dir.join("new_clip.mp4");
        fs::write(&video_path, b"pretend video bytes").unwrap();

        // Real debounce + real OS event delivery both take a moment; give
        // this test generous real headroom rather than a hair-trigger
        // timeout (STABILITY_CHECKS_REQUIRED * STABILITY_POLL_INTERVAL is
        // ~1s once the event is actually delivered).
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let found = seen.lock().unwrap();
        assert_eq!(found.len(), 1, "expected exactly one arrival callback");
        assert_eq!(found[0], video_path);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn watch_folder_ignores_a_non_video_extension() {
        let dir = temp_dir("ignore-ext");
        let seen: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_for_cb = seen.clone();
        let _watcher = watch_folder(&dir, move |path| {
            seen_for_cb.lock().unwrap().push(path);
        })
        .expect("starting a real watch on a real temp dir");

        fs::write(dir.join("notes.txt"), b"not a video").unwrap();

        std::thread::sleep(Duration::from_secs(2));
        assert!(
            seen.lock().unwrap().is_empty(),
            "a .txt file must never trigger a watch-folder arrival"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
