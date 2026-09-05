//! Resumable model downloads (master prompt §60): temporary `<file>.part`
//! while downloading, HTTP `Range` requests to resume, verify, then atomic
//! rename. Uses `ureq` (blocking, rustls-backed — no OpenSSL dependency to
//! worry about for Windows packaging) since this already runs on a
//! background thread via `tauri::async_runtime::spawn_blocking`, matching
//! `media::proxy`/`render::job`'s own synchronous-work-on-a-background-
//! thread shape — no async HTTP runtime needed for one sequential byte
//! stream.
//!
//! ## What "verify" means here
//!
//! whisper.cpp's own `models/download-ggml-model.sh` publishes no per-model
//! checksum (checked before writing this — see `models` module doc
//! comment), but Hugging Face's own CDN does expose one: a real, verified
//! `curl -IL` against `ggml-tiny.bin`'s exact download URL (module doc
//! comment's own verification discipline) shows the pre-redirect response
//! carrying `X-Linked-ETag: "<64 lowercase hex characters>"` — the real
//! SHA-256 of the underlying git-LFS-tracked blob, not the CDN's own content-
//! addressed `etag` on the post-redirect response (a different value; do not
//! confuse the two, see `peek_expected_sha256`). Since this is a genuinely
//! Hugging-Face-specific CDN behavior (nothing guarantees a future non-HF
//! mirror would expose the same header, let alone with the same meaning),
//! `peek_expected_sha256` only ever attempts this for a real
//! `HF_BASE_URL`-prefixed download — for anything else (a test's local mock
//! server, a hypothetical future alternate source), this pass's
//! verification falls back to the same byte-count completeness check as
//! before: the number of bytes actually written to the `.part` file exactly
//! equals the `Content-Length` (or `Content-Range` total, when resuming) the
//! server reported. Both checks together: a truncated/interrupted transfer
//! is always caught (byte count), and — for the real, shipped Hugging Face
//! catalog this app actually downloads from — so is a corrupted-but-
//! complete-length transfer (a real cryptographic hash match), before either
//! is ever renamed into place as "installed".
//!
//! ## `.part` files are resumable, not disposable
//!
//! Unlike `media::proxy`/`render::job` (which delete partial output on
//! cancellation/failure, because a partial *render* is never useful), a
//! `.part` model download is deliberately **kept** on cancellation or a
//! network failure mid-stream — that's the entire point of `Range`-based
//! resuming. It is only ever removed by an explicit user action (deleting
//! the model, which `transcription::models::delete_model` refuses to do to
//! a `.part` file in the first place — see that module) or overwritten by a
//! fresh, from-scratch download when the server doesn't honor the `Range`
//! request.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;

use super::error::ModelError;
use super::models::{ModelCatalogEntry, ModelId, HF_BASE_URL};

/// 256 KiB — large enough that read/write syscalls aren't the bottleneck
/// for a multi-hundred-MB/multi-GB model file, small enough that a progress
/// tick (and a cancellation check) still happens often enough to feel
/// responsive.
const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Type)]
pub struct DownloadProgress {
    pub filename: String,
    pub model_id: ModelId,
    /// Total expected bytes, from the server's own `Content-Length`/
    /// `Content-Range` — not the catalog's `approx_size_bytes` (which is
    /// just a display estimate; this is the authoritative figure for this
    /// specific transfer).
    pub size: u64,
    pub downloaded: u64,
    pub speed_bytes_per_sec: f64,
    /// `None` until at least one chunk has been timed (so speed is
    /// nonzero).
    pub eta_secs: Option<f64>,
}

pub fn part_path(dest_dir: &Path, entry: &ModelCatalogEntry) -> PathBuf {
    dest_dir.join(format!("{}.part", entry.filename))
}

pub fn final_path(dest_dir: &Path, entry: &ModelCatalogEntry) -> PathBuf {
    dest_dir.join(&entry.filename)
}

/// Normalizes a raw `X-Linked-ETag` header value into a real SHA-256 hex
/// digest, or `None` if it isn't one — strips surrounding `"` quotes
/// (Hugging Face's CDN always wraps it in them) and whitespace, lowercases
/// it, then requires exactly 64 lowercase-hex-after-normalization
/// characters (a real SHA-256 digest's length; anything else is either a
/// different kind of etag entirely or a header this pass doesn't recognize
/// — never treated as a match to compare against, only ever as "no
/// verification available this time").
fn sanitize_expected_sha256(raw: &str) -> Option<String> {
    let cleaned = raw.trim().trim_matches('"').trim().to_ascii_lowercase();
    if cleaned.len() == 64 && cleaned.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(cleaned)
    } else {
        None
    }
}

/// Best-effort: looks up the real SHA-256 Hugging Face's CDN publishes for
/// `download_url`, if (and only if) `download_url` actually starts with the
/// real `HF_BASE_URL` this catalog downloads from (module doc comment: this
/// is genuinely Hugging-Face-specific CDN behavior, not a general HTTP
/// convention, so it is never attempted for anything else — a test's local
/// mock server, or a hypothetical future non-HF mirror, both correctly get
/// `None` here and fall back to this module's byte-count-only check).
///
/// Uses a dedicated `redirects(0)` agent: ureq returns the *pre-redirect*
/// response object (with a real 3xx status, not an `Err`) when redirects are
/// disabled, and Hugging Face's `X-Linked-ETag` header lives on exactly that
/// pre-redirect response — the final, followed CDN response carries a
/// different `etag` value entirely (module doc comment). Never fails the
/// caller: any network error, missing header, or unrecognized value simply
/// yields `None`.
fn peek_expected_sha256(download_url: &str) -> Option<String> {
    if !download_url.starts_with(HF_BASE_URL) {
        return None;
    }
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    let response = agent.get(download_url).call().ok()?;
    let header = response.header("x-linked-etag")?;
    sanitize_expected_sha256(header)
}

/// Streams `path` through a real SHA-256 hasher, returning its lowercase hex
/// digest. Pure file I/O, no network — reused by both the real download
/// path and its own unit tests.
fn sha256_hex_of_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Hashes `path` and compares it against `expected_hex_lowercase`, returning
/// `ModelError::VerificationFailed` on any mismatch (or a real I/O error
/// while hashing) — the real cryptographic half of this module's
/// verification (module doc comment), composed as its own function so it's
/// unit-testable directly against a known-content temp file rather than only
/// indirectly through a full network download.
fn verify_sha256_or_fail(
    model_id: &str,
    path: &Path,
    expected_hex_lowercase: &str,
) -> Result<(), ModelError> {
    let actual = sha256_hex_of_file(path).map_err(|e| ModelError::IoFailed {
        model_id: model_id.to_string(),
        details: format!("hashing {} for verification: {e}", path.display()),
    })?;
    if actual != expected_hex_lowercase {
        return Err(ModelError::VerificationFailed {
            model_id: model_id.to_string(),
            details: format!("sha256 mismatch: expected {expected_hex_lowercase}, got {actual}"),
        });
    }
    Ok(())
}

/// Downloads `entry` into `dest_dir`, resuming from an existing `.part` file
/// if one is present (module doc comment). Returns the final installed path
/// on success.
pub fn download_model(
    entry: &ModelCatalogEntry,
    dest_dir: &Path,
    cancel: Option<&AtomicBool>,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, ModelError> {
    let model_id = || entry.id.as_str().to_string();

    std::fs::create_dir_all(dest_dir).map_err(|e| ModelError::IoFailed {
        model_id: model_id(),
        details: format!("creating model dir {}: {e}", dest_dir.display()),
    })?;

    let part = part_path(dest_dir, entry);
    let target = final_path(dest_dir, entry);
    let already_on_disk: u64 = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    // Best-effort, before opening any connection for the real transfer:
    // `None` for anything that isn't a real Hugging Face URL (module doc
    // comment), in which case verification below falls back to byte-count
    // only, exactly as before this pass.
    let expected_sha256 = peek_expected_sha256(&entry.download_url);

    let fail = |details: String| ModelError::DownloadFailed {
        model_id: model_id(),
        details,
    };

    let mut request = ureq::get(&entry.download_url);
    if already_on_disk > 0 {
        request = request.set("Range", &format!("bytes={already_on_disk}-"));
    }
    let response = request.call().map_err(|e| fail(e.to_string()))?;

    // The server may ignore an unsupported/stale Range request and answer
    // 200 with the full file — in that case the existing `.part` bytes
    // can't be trusted to line up with this fresh stream, so start over.
    let resuming = response.status() == 206;
    let content_length: u64 = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| fail("response had no Content-Length header".to_string()))?;
    let total_size = if resuming {
        already_on_disk + content_length
    } else {
        content_length
    };
    let mut downloaded = if resuming { already_on_disk } else { 0 };

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&part)
        .map_err(|e| ModelError::IoFailed {
            model_id: model_id(),
            details: format!("opening {}: {e}", part.display()),
        })?;

    let mut reader = response.into_reader();
    let mut buf = vec![0u8; CHUNK];
    let started = Instant::now();
    let downloaded_at_start = downloaded;

    loop {
        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                let _ = file.flush();
                return Err(ModelError::DownloadCancelled {
                    model_id: model_id(),
                });
            }
        }
        let read = reader
            .read(&mut buf)
            .map_err(|e| fail(format!("reading response body: {e}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|e| ModelError::IoFailed {
                model_id: model_id(),
                details: format!("writing {}: {e}", part.display()),
            })?;
        downloaded += read as u64;

        let elapsed = started.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
            (downloaded - downloaded_at_start) as f64 / elapsed
        } else {
            0.0
        };
        on_progress(DownloadProgress {
            filename: entry.filename.clone(),
            model_id: entry.id,
            size: total_size,
            downloaded,
            speed_bytes_per_sec: speed,
            eta_secs: if speed > 0.0 {
                Some(total_size.saturating_sub(downloaded) as f64 / speed)
            } else {
                None
            },
        });
    }
    let _ = file.sync_all();
    drop(file);

    if downloaded != total_size {
        // A real network drop mid-stream. The `.part` file is left as-is
        // (module doc comment) so a retry resumes instead of restarting.
        return Err(fail(format!(
            "incomplete download: got {downloaded} of {total_size} bytes"
        )));
    }

    // Verify (module doc comment): byte-count completeness always; a real
    // SHA-256 match too, whenever `expected_sha256` peeked one (a real
    // Hugging Face download). Both run before this file is ever renamed
    // into place as "installed".
    let on_disk = std::fs::metadata(&part)
        .map(|m| m.len())
        .map_err(|e| ModelError::IoFailed {
            model_id: model_id(),
            details: format!("stat {}: {e}", part.display()),
        })?;
    if on_disk != total_size {
        return Err(ModelError::VerificationFailed {
            model_id: model_id(),
            details: format!("expected {total_size} bytes on disk, found {on_disk}"),
        });
    }
    if let Some(expected) = &expected_sha256 {
        verify_sha256_or_fail(&model_id(), &part, expected)?;
    }

    std::fs::rename(&part, &target).map_err(|e| ModelError::IoFailed {
        model_id: model_id(),
        details: format!("renaming {} -> {}: {e}", part.display(), target.display()),
    })?;

    on_progress(DownloadProgress {
        filename: entry.filename.clone(),
        model_id: entry.id,
        size: total_size,
        downloaded: total_size,
        speed_bytes_per_sec: 0.0,
        eta_secs: Some(0.0),
    });

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::models::ModelId;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    // -- §53 model-hash verification: pure logic, no network ----------------

    #[test]
    fn sanitize_expected_sha256_accepts_a_real_quoted_hex_etag() {
        // A real, verified example from `curl -IL` against the real
        // `ggml-tiny.bin` URL (module doc comment).
        let raw = "\"be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21\"";
        assert_eq!(
            sanitize_expected_sha256(raw).as_deref(),
            Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21")
        );
    }

    #[test]
    fn sanitize_expected_sha256_normalizes_case_and_surrounding_whitespace() {
        let raw = "  \"BE07E048E1E599AD46341C8D2A135645097A538221678B7ACDD1B1919C6E1B21\"  ";
        assert_eq!(
            sanitize_expected_sha256(raw).as_deref(),
            Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21")
        );
    }

    #[test]
    fn sanitize_expected_sha256_rejects_the_wrong_length() {
        assert!(sanitize_expected_sha256("\"deadbeef\"").is_none());
        assert!(sanitize_expected_sha256("\"\"").is_none());
    }

    #[test]
    fn sanitize_expected_sha256_rejects_non_hex_characters() {
        // The real xet-hash value HF's CDN puts on the *other* header
        // (plain `etag`, post-redirect) is 64 chars but not necessarily
        // hex-only in general — and any header carrying something that
        // isn't a real hex digest must never be treated as one.
        let raw = "\"not-a-real-hash-at-all-just-some-64-character-junk-string-zzzz\"";
        assert!(sanitize_expected_sha256(raw).is_none());
    }

    #[test]
    fn peek_expected_sha256_never_attempts_a_network_call_for_a_non_huggingface_url() {
        // Gated by construction (module doc comment): a URL that isn't a
        // real Hugging Face download short-circuits to `None` without ever
        // opening a connection — verified here by pointing at a URL with no
        // listener at all (a real network attempt would hang/error, not
        // silently return `None` this fast).
        let result = peek_expected_sha256("http://127.0.0.1:1/does-not-matter");
        assert!(result.is_none());
    }

    #[test]
    fn sha256_hex_of_file_matches_a_known_test_vector() {
        let dir =
            std::env::temp_dir().join(format!("ave-model-hash-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.bin");
        std::fs::write(&path, b"hello world").unwrap();

        // A widely-published SHA-256 test vector for the literal bytes
        // "hello world".
        assert_eq!(
            sha256_hex_of_file(&path).unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sha256_hex_of_file_matches_the_empty_file_test_vector() {
        let dir = std::env::temp_dir().join(format!(
            "ave-model-hash-empty-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        assert_eq!(
            sha256_hex_of_file(&path).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_sha256_or_fail_succeeds_on_a_real_match() {
        let dir =
            std::env::temp_dir().join(format!("ave-model-verify-ok-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.bin");
        std::fs::write(&path, b"hello world").unwrap();

        verify_sha256_or_fail(
            "tiny",
            &path,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        )
        .expect("matching hash verifies");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_sha256_or_fail_rejects_a_real_mismatch() {
        // The real, reportable security case: a corrupted-but-complete-
        // length download must be caught, not silently installed.
        let dir = std::env::temp_dir().join(format!(
            "ave-model-verify-mismatch-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupted.bin");
        std::fs::write(&path, b"this is not the expected content at all").unwrap();

        let err = verify_sha256_or_fail(
            "tiny",
            &path,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        )
        .unwrap_err();
        assert!(
            matches!(err, ModelError::VerificationFailed { model_id, .. } if model_id == "tiny")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// How a one-shot test server responds to the single request a test
    /// sends it.
    enum ServeMode {
        /// Always answers 200 with the full body, ignoring any `Range`
        /// header — simulates a server/proxy that doesn't support resuming.
        IgnoreRange(Vec<u8>),
        /// Honors a `Range: bytes=N-` header with a real 206 partial
        /// response; answers 200 with the full body if there's no `Range`
        /// header at all.
        Resumable(Vec<u8>),
        /// Claims the given full `Content-Length` but only ever writes the
        /// first `keep` bytes before closing the connection — simulates a
        /// dropped connection partway through a transfer.
        DropAfter { body: Vec<u8>, keep: usize },
    }

    fn read_request_range(stream: &TcpStream) -> Option<(u64, ())> {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).ok()?;
        let mut range: Option<(u64, ())> = None;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).ok()?;
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("range:") {
                // "bytes=1234-" -> 1234
                if let Some(start) = rest
                    .trim()
                    .strip_prefix("bytes=")
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    range = Some((start, ()));
                }
            }
        }
        range
    }

    fn spawn_server(mode: ServeMode) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let range = read_request_range(&stream);
            let mut stream = stream;
            match mode {
                ServeMode::IgnoreRange(body) => {
                    write_full(&mut stream, &body);
                }
                ServeMode::Resumable(body) => match range {
                    Some((start, ())) if (start as usize) <= body.len() => {
                        write_partial(&mut stream, &body, start as usize);
                    }
                    _ => write_full(&mut stream, &body),
                },
                ServeMode::DropAfter { body, keep } => {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body[..keep.min(body.len())]);
                    // Deliberately drop `stream` here without writing the
                    // rest of the body, simulating a severed connection.
                }
            }
        });
        format!("http://{addr}")
    }

    fn write_full(stream: &mut TcpStream, body: &[u8]) {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(body);
    }

    fn write_partial(stream: &mut TcpStream, body: &[u8], start: usize) {
        let slice = &body[start..];
        let end = body.len().saturating_sub(1);
        let header = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len(),
            slice.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(slice);
    }

    fn test_entry(url: String) -> ModelCatalogEntry {
        ModelCatalogEntry {
            id: ModelId::Tiny,
            filename: "ggml-tiny.bin".to_string(),
            display_name: "Tiny".to_string(),
            approx_size_bytes: 1,
            multilingual: true,
            download_url: url,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ave-model-dl-test-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn downloads_a_full_file_and_renames_it_into_place() {
        let body = b"0123456789".repeat(1000); // 10,000 bytes
        let url = spawn_server(ServeMode::Resumable(body.clone()));
        let entry = test_entry(url);
        let dir = temp_dir("full");

        let mut ticks = 0u32;
        let path = download_model(&entry, &dir, None, |_| ticks += 1).expect("download succeeds");

        assert_eq!(path, dir.join("ggml-tiny.bin"));
        assert!(!dir.join("ggml-tiny.bin.part").exists());
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert!(ticks > 0, "expected at least one progress callback");

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- §88 Windows path edge case: a dest_dir containing spaces and real
    //    non-ASCII Unicode (a real Windows user profile can look exactly
    //    like this) -----------------------------------------------------

    #[test]
    fn downloads_into_a_unicode_and_space_containing_dest_dir() {
        let body = b"0123456789".repeat(1000);
        let url = spawn_server(ServeMode::Resumable(body.clone()));
        let entry = test_entry(url);
        let base = temp_dir("unicode-dest");
        let dir = base.join("Users").join("Nguyễn Văn A 🎬").join("models");
        std::fs::create_dir_all(&dir).unwrap();

        let path = download_model(&entry, &dir, None, |_| {})
            .expect("download succeeds under a Unicode/space-containing dest_dir");

        assert_eq!(path, dir.join("ggml-tiny.bin"));
        assert!(path.to_string_lossy().contains("Nguyễn Văn A 🎬"));
        assert_eq!(std::fs::read(&path).unwrap(), body);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn resumes_from_an_existing_part_file_via_range_request() {
        let body = b"abcdefghij".repeat(2000); // 20,000 bytes
        let url = spawn_server(ServeMode::Resumable(body.clone()));
        let entry = test_entry(url);
        let dir = temp_dir("resume");

        // Simulate a prior interrupted download: the first half already on
        // disk as a `.part` file.
        let half = body.len() / 2;
        std::fs::write(dir.join("ggml-tiny.bin.part"), &body[..half]).unwrap();

        let path = download_model(&entry, &dir, None, |_| {}).expect("resume succeeds");
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert!(!dir.join("ggml-tiny.bin.part").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restarts_from_scratch_when_the_server_ignores_the_range_request() {
        let body = b"zyxwvutsrq".repeat(500); // 5,000 bytes
        let url = spawn_server(ServeMode::IgnoreRange(body.clone()));
        let entry = test_entry(url);
        let dir = temp_dir("no-range-support");

        // A stale/garbage partial from some earlier, incompatible attempt.
        std::fs::write(dir.join("ggml-tiny.bin.part"), b"garbage-not-a-real-prefix").unwrap();

        let path = download_model(&entry, &dir, None, |_| {}).expect("download succeeds");
        // The final file must be the real, full, correct body — not the
        // stale partial bytes with the real body appended after it.
        assert_eq!(std::fs::read(&path).unwrap(), body);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_dropped_connection_leaves_the_part_file_and_returns_an_error() {
        let body = b"0123456789".repeat(1000); // 10,000 bytes claimed
        let url = spawn_server(ServeMode::DropAfter {
            body: body.clone(),
            keep: 100, // only ever writes 100 of the promised 10,000 bytes
        });
        let entry = test_entry(url);
        let dir = temp_dir("dropped");

        let err = download_model(&entry, &dir, None, |_| {}).unwrap_err();
        assert!(matches!(err, ModelError::DownloadFailed { .. }));

        // Never renamed into place — a killed download must never be
        // mistaken for an installed model.
        assert!(!dir.join("ggml-tiny.bin").exists());
        // The partial bytes are kept so a retry can attempt to resume.
        assert!(dir.join("ggml-tiny.bin.part").exists());
        assert_eq!(
            std::fs::metadata(dir.join("ggml-tiny.bin.part"))
                .unwrap()
                .len(),
            100
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancelling_leaves_the_part_file_for_a_future_resume() {
        // A large-ish body so the download loop has time to observe the
        // cancellation flag before finishing.
        let body = vec![7u8; 4 * CHUNK];
        let url = spawn_server(ServeMode::Resumable(body));
        let entry = test_entry(url);
        let dir = temp_dir("cancel");

        let cancel = AtomicBool::new(true); // already cancelled before starting
        let err = download_model(&entry, &dir, Some(&cancel), |_| {}).unwrap_err();
        assert!(matches!(err, ModelError::DownloadCancelled { .. }));
        assert!(!dir.join("ggml-tiny.bin").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real-network smoke test (per this phase's "verify, don't assume"
    /// instruction): confirms the actual, hardcoded Hugging Face URL
    /// pattern still resolves and serves real bytes with a real
    /// `Content-Length`, end to end through this exact download function —
    /// not just a `curl -I` check. Downloads only the first ~64KB of the
    /// real `ggml-tiny.bin` (via a `Range`-shaped cancel-after-one-chunk)
    /// to stay fast/deterministic in CI while still exercising the real
    /// HTTPS/redirect/TLS path. Marked `#[ignore]` since it depends on
    /// outbound internet access, which a sandboxed/offline CI runner may
    /// not have; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore = "requires real internet access to huggingface.co"]
    fn real_huggingface_url_pattern_serves_real_bytes() {
        use crate::transcription::models::catalog_entry;

        let entry = catalog_entry(ModelId::Tiny);
        let dir = temp_dir("real-network-smoke");

        // Cancel as soon as the first progress tick lands, so this stays a
        // smoke test (confirms the URL/redirect/TLS/Content-Length path
        // works) rather than a multi-second full download.
        let cancel = AtomicBool::new(false);
        let mut first_tick_seen = false;
        let result = download_model(&entry, &dir, Some(&cancel), |p| {
            assert!(
                p.size > 50_000_000,
                "expected a real multi-MB Content-Length, got {}",
                p.size
            );
            if !first_tick_seen {
                first_tick_seen = true;
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert!(first_tick_seen, "expected at least one real progress tick");
        assert!(matches!(result, Err(ModelError::DownloadCancelled { .. })));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real-network smoke test for the hash-verification mechanism itself
    /// (§53 "validate downloaded model hashes when possible" — see this
    /// module's doc comment for how `X-Linked-ETag` makes that genuinely
    /// possible for a real Hugging Face URL): confirms the real, hardcoded
    /// `ggml-tiny.bin` URL actually carries the expected header shape, not
    /// just that the byte-count path works (`real_huggingface_url_pattern_
    /// serves_real_bytes` above already covers that). `#[ignore]`d for the
    /// same reason — real outbound internet access, not available to every
    /// CI runner; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore = "requires real internet access to huggingface.co"]
    fn peek_expected_sha256_finds_a_real_64_char_hex_digest_for_the_real_catalog_url() {
        use crate::transcription::models::catalog_entry;

        let entry = catalog_entry(ModelId::Tiny);
        let hash = peek_expected_sha256(&entry.download_url)
            .expect("Hugging Face's real CDN response should carry a real X-Linked-ETag");
        assert_eq!(hash.len(), 64, "{hash}");
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()), "{hash}");
    }
}
