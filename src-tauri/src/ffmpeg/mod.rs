//! FFmpeg/ffprobe integration: sidecar binary resolution (`binaries`) and the
//! process-argument-array command builder (`command`) that every ffmpeg
//! caller in this crate (media probe/thumbnail/proxy, audio PCM extraction,
//! and — Phase 6 — rendering) goes through instead of ad hoc
//! `Command::new(...).args(...)` calls with hand-built strings (master
//! prompt §66/§88).

pub mod binaries;
pub mod command;
