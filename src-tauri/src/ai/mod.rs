//! AI edit-plan architecture (Phase 10, `IMPLEMENTATION_PLAN.md`, master
//! prompt §17/§18): the `AIProvider` trait + concrete adapters, secure
//! credential storage, and the `EditPlan` schema + strict validation.
//!
//! `provider`/`openai_compat`/`anthropic`/`gemini`/`credentials`/`edit_plan`
//! are this subsystem's **foundation**: "how do we talk to *some* LLM
//! provider" and "how do we turn whatever text an LLM hands back into
//! something safe to feed the timeline engine". `smart_edit`, `nl_command`,
//! and `media_tags` are features built on top of that foundation:
//! `smart_edit` is Smart Edit / AI semantic editing (master prompt §19);
//! `nl_command` is the natural-language AI command box's prompt-construction
//! layer (master prompt §20, `commands::ai::generate_edit_plan_from_nl_command`);
//! `media_tags` is master prompt §35's "Optional AI-generated tags"
//! enhancement on top of Phase 3's already-built media library. Real
//! highlight detection (master prompt §21) and B-roll (master prompt §34)
//! live in the separate `crate::highlights`/`crate::broll` modules, since
//! each needs its own non-AI signal/source machinery of its own
//! (`vad`/`audio`/`media::scene` for highlights; `crate::db` for B-roll)
//! alongside an optional AI call, not just a prompt in front of `edit_plan`.
//!
//! **Security note (master prompt §53):** the AI layer never mutates the
//! timeline directly and never executes anything an LLM returns as code —
//! `edit_plan::EditOperation` is a closed, strictly-typed Rust enum (not a
//! free-form string `type` field interpreted dynamically), so an unknown
//! operation kind simply fails to deserialize; there is no path from AI
//! output to arbitrary code/shell execution by construction. Approved
//! `Remove` operations are applied only through the existing
//! `timeline::command`/`timeline::silence` machinery
//! (`commands::ai::apply_edit_plan_to_clip`/`apply_edit_plan_to_track`),
//! exactly the same "propose then explicitly apply, as one atomic undo
//! step" shape every prior phase (VAD, filler-word, captions) already uses.

pub mod anthropic;
pub mod credentials;
pub mod edit_plan;
pub mod error;
pub mod gemini;
pub mod media_tags;
pub mod nl_command;
pub mod openai_compat;
pub mod provider;
pub mod smart_edit;

/// Shared one-shot mock HTTP server for adapter tests (`openai_compat`,
/// `anthropic`, `gemini`), following `transcription::download`'s own
/// "real HTTP client behind a trait, tested against a local mock server"
/// precedent (module doc comment there) rather than a fake/mocked
/// `AIProvider`. Kept in one place instead of copy-pasted three times.
#[cfg(test)]
pub(crate) mod test_http {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;

    /// What the mock server actually received, for tests that want to assert
    /// on the exact request an adapter built (method, path, headers, body).
    #[derive(Debug, Clone)]
    pub struct CapturedRequest {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl CapturedRequest {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    /// Spawns a one-shot HTTP server bound to an ephemeral localhost port:
    /// reads exactly one request, sends what it captured back over the
    /// returned channel, then responds with `status_line` (e.g.
    /// `"HTTP/1.1 200 OK"`) and `body` (as `application/json`). Returns the
    /// server's base URL (`http://127.0.0.1:PORT`, with no trailing slash)
    /// so a test can build `format!("{base}/v1/chat/completions")` etc.
    pub fn spawn_one_shot(
        status_line: &'static str,
        body: String,
    ) -> (String, mpsc::Receiver<CapturedRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut stream = stream;
            let captured = read_request(&mut stream);
            let _ = tx.send(captured);
            let header = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body.as_bytes());
        });
        (format!("http://{addr}"), rx)
    }

    /// Spawns a listener that accepts and immediately drops the connection
    /// with no response at all — simulates an unreachable/misbehaving
    /// endpoint for "connection test correctly reports failure" cases.
    pub fn spawn_connection_refused() -> String {
        // A real "nothing is listening here" address: bind then immediately
        // drop the listener, freeing the port but leaving nothing to accept
        // a connection on it.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);
        format!("http://{addr}")
    }

    fn read_request(stream: &mut TcpStream) -> CapturedRequest {
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap_or(0);
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();

        let mut headers = Vec::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).unwrap_or(0);
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                if k.eq_ignore_ascii_case("content-length") {
                    content_length = v.parse().unwrap_or(0);
                }
                headers.push((k, v));
            }
        }
        let mut body_buf = vec![0u8; content_length];
        if content_length > 0 {
            let _ = reader.read_exact(&mut body_buf);
        }
        CapturedRequest {
            method,
            path,
            headers,
            body: String::from_utf8_lossy(&body_buf).to_string(),
        }
    }
}
