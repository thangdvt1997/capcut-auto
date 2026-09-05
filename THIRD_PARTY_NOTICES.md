# Third-Party Notices

This product incorporates code and design derived from third-party open-source projects. Per master prompt §73, each is tracked separately below — see `docs/upstream.md` for which modules in this repo originate from which project.

---

## pyJianYingDraft / capcut-mate

Portions of `src-tauri/src/capcut/` are ported from `pyJianYingDraft`, incorporated into and modified within the `capcut-mate` repository (https://github.com/Hommy-master/capcut-mate).

```
pyJianYingDraft
Copyright 2024 Gary Guan
Licensed under the Apache License, Version 2.0.

This project includes modifications and additional code created by Hommy.
Copyright 2026 Hommy <taohongmin@sina.cn>.
Also licensed under the Apache License, Version 2.0.
```

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0.

**Modifications made in this project**: structural port from Python to Rust (`src-tauri/src/capcut/`), preserving the microsecond timebase and materials-collected-on-add-segment semantics. Tracked in detail in `docs/upstream.md`.

---

## capcut-mate desktop-client (draft path detection)

The Windows/macOS installed-app path-detection heuristic in this project's CapCut/Jianying detection module is informed by `desktop-client/nodeapi/draftPathDetect.js` from the `capcut-mate` repository (https://github.com/Hommy-master/capcut-mate), a separately licensed sub-tree:

```
MIT License

Copyright (c) 2025 gogoshine

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

---

## autocut

Portions of `src-tauri/src/media/`, `src-tauri/src/audio/`, `src-tauri/src/vad/`, `src-tauri/src/render/` (concat-based MP4 export), and `src-tauri/src/fcpxml/` are reimplemented/ported from `autocut` (https://github.com/cobanov/autocut).

**Phase 2 additions (direct reuse, not just design reference):**
- `src/components/layout/ResizableSplit.svelte` — ported verbatim from `vendor/autocut/src/components/ResizableSplit.svelte` (localStorage-persisted split-pane component).
- `src-tauri/src/main.rs`'s `windows_subsystem = "windows"` release-console-suppression pattern, and `src-tauri/src/lib.rs`'s `#[cfg(not(test))]` exclusion of `run()` from the lib's test build (both from `vendor/autocut/src-tauri/src/{main,lib}.rs`).
- `.gitignore`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json` structure, and the `.github/workflows/ci.yml` two-job (frontend/Rust) shape — adapted from autocut's config file patterns, per the project owner's permission to reuse autocut's Cargo dependency choices, config patterns, `.gitignore`, and CI workflow structure directly (see `docs/upstream.md`).
- `src-tauri/icons/*` — the placeholder icon set is autocut's own stock Tauri-CLI-generated default icons, copied as a bootstrap placeholder pending real app branding (not autocut-original artwork; the Tauri CLI generates the same default set for any new project).

**License status**: the `autocut` repository itself carries no LICENSE file and no license grant (`"license": null` per the GitHub API, confirmed 2026-09-04 — see `docs/architecture-audit.md` §7). The project owner has stated permission was obtained directly from the author, Mert Cobanov, to reuse this code. No separate license text is reproduced here because none exists upstream to reproduce; attribution is recorded instead:

> Built on design and code from `autocut` by Mert Cobanov (https://github.com/cobanov/autocut), used with the author's permission.

If a formal license is ever added upstream, replace this notice with the actual license text.

---

## Silero VAD (`voice_activity_detector` crate)

The voice-activity-detection engine uses the `voice_activity_detector` Rust crate (https://crates.io/crates/voice_activity_detector), an independent dependency pulled directly from crates.io regardless of `autocut`'s own licensing status — see its own license on crates.io/its repository at build-dependency-lock time.

---

## FFmpeg / FFprobe

*To be finalized in Phase 12 (master prompt §59) once the exact bundled build/version is chosen. Must document: source, version, license variant (LGPL vs GPL — affects whether GPL-only codecs are enabled), and confirm build flags before shipping.*

---

## Other bundled third-party components

Direct dependencies as of Phase 12 (2026-09-05), by ecosystem. All are permissively licensed (MIT and/or Apache-2.0, or public domain) — none require verbatim license-text reproduction in this file under standard open-source practice, but are listed here for completeness. **This list covers direct dependencies only** — `Cargo.lock`/`pnpm-lock.yaml` pull in many more transitive dependencies; before a real public release, run `cargo install cargo-about && cargo about generate` (or `cargo license`) against `src-tauri/Cargo.lock` and `pnpm licenses list` against the frontend lockfile to produce a complete, tool-generated transitive report, and attach that report's output here (or as a linked appendix) rather than relying on this manual list alone.

**Rust crates** (`src-tauri/Cargo.toml`): `tauri`, `tauri-build`, `tauri-plugin-dialog`, `tauri-specta`, `specta`, `specta-typescript` (Tauri project, MIT/Apache-2.0 dual) · `serde`, `serde_json` (dtolnay/serde-rs, MIT/Apache-2.0 dual) · `anyhow`, `thiserror` (dtolnay, MIT/Apache-2.0 dual) · `uuid` (uuid-rs, MIT/Apache-2.0 dual) · `chrono` (chronotope, MIT/Apache-2.0 dual) · `rusqlite` (rusqlite project, MIT; built with the `bundled` feature, which statically links SQLite itself — SQLite is dedicated to the public domain by its authors) · `ureq` (algesten, MIT/Apache-2.0 dual) · `keyring` (hwakabayashi/open-source contributors, MIT/Apache-2.0 dual; on Windows this wraps the real Win32 Credential Manager API, not a third-party service) · `winreg` (gentoo90, MIT/Apache-2.0 dual) · `voice_activity_detector` (see its own dedicated section above) · `whisper-rs` (tazz4843/whisper-rs project, MIT/Apache-2.0 dual; itself a binding around `whisper.cpp` by Georgi Gerganov, MIT-licensed, vendored and compiled at build time by `whisper-rs`'s own build script — no separate copy is committed to this repo).

**npm/frontend packages** (`package.json`): `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@tauri-apps/cli` (Tauri project, MIT/Apache-2.0 dual) · `svelte`, `@sveltejs/vite-plugin-svelte` (Svelte core team, MIT) · `vite` (Evan You/Vite team, MIT) · `typescript`, `typescript-eslint` (Microsoft/typescript-eslint team, Apache-2.0/MIT respectively) · `eslint`, `eslint-plugin-svelte`, `svelte-eslint-parser`, `@eslint/js` (OpenJS Foundation/community, MIT) · `svelte-check` (Svelte team, MIT) · `globals`, `tslib`, `@tsconfig/svelte` (community/Microsoft, MIT).

**Fonts/icons**: no third-party font files are bundled — UI text uses system/web-safe font stacks declared in CSS, not embedded font assets. `src-tauri/icons/*` remains the Tauri-CLI-generated placeholder set noted in the `autocut` section above, pending real app-branding artwork before a public release.

Run a license-audit pass at the end of each phase per `IMPLEMENTATION_PLAN.md`, and the full `cargo-about`/`pnpm licenses` tool-generated pass specifically before any public/signed release build.
