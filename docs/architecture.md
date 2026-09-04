# Architecture — AI Video Editor

High-level companion to `docs/architecture-audit.md` (which explains *why*, citing upstream code). This document is the forward-looking design: what the unified app's own components are and how they talk to each other.

## Component diagram

```
                         ┌────────────────────────────┐
                         │        Desktop UI           │
                         │  (Svelte 5 + TS, src/)      │
                         │  Project Mgr · Media Lib ·   │
                         │  Preview · Timeline ·        │
                         │  Transcript · Captions ·     │
                         │  AI Editor · Export ·        │
                         │  Settings                    │
                         └──────────────┬───────────────┘
                                        │ Tauri IPC (typed, specta-generated)
                         ┌──────────────▼───────────────┐
                         │           Rust Core           │
                         │         (src-tauri/src/)      │
                         │                                │
   ┌──────────┐   ┌──────┴──────┐   ┌──────────┐   ┌──────▼─────┐
   │  media/   │   │  timeline/  │   │  render/  │   │  capcut/    │
   │  ffprobe  │   │  clips/     │   │ RenderGraph│  │ ScriptFile  │
   │  audio/   │◄──┤  tracks/    ├──►│ → FFmpeg   │  │ → Draft     │
   │  vad/     │   │  undo/redo  │   │  plan      │  │ (Apache-2.0 │
   │  waveform │   │  SyncGroup  │   └────────────┘  │  port)      │
   └──────────┘   └──────┬──────┘   ┌────────────┐  └────────────┘
                         │           │  fcpxml/    │
                  ┌──────▼──────┐   │  → DaVinci  │
                  │     ai/      │   └────────────┘
                  │ EditPlan     │
                  │ validation   │
                  └──────────────┘
                         │
                  ┌──────▼──────┐   ┌────────────┐   ┌────────────┐
                  │  project/    │   │  jobs/      │   │    db/      │
                  │ ProjectV1    │   │ JobManager  │   │ SQLite:     │
                  │ atomic save  │   │ cancellation│   │ media index,│
                  │ migration    │   │ progress    │   │ recents,    │
                  └──────────────┘   └────────────┘   │ models, jobs│
                                                        └────────────┘
```

## Why this shape

- **Local IPC only, no HTTP server, no Docker.** capcut-mate's FastAPI layer is not reused (master prompt §2, audit §5). Every Rust module above is called via `#[tauri::command]`, never over a network socket.
- **AI never touches the timeline directly.** `ai/` only produces a validated `EditPlan` JSON document; only `timeline/`'s own command layer mutates project state (master prompt §18/§82). See the pipeline diagram in `docs/ai-engine.md` (Phase 10).
- **Three independent export adapters share one timeline.** `render/` (local FFmpeg), `capcut/` (draft export), `fcpxml/` (DaVinci/Premiere) are siblings, not a hierarchy — none of them is "the" renderer (master prompt §69/§70/§81). A `docs/feature-matrix.md` (Phase 9) tracks what each adapter can/can't represent.
- **`timeline/` is new, not inherited.** Neither upstream repo's data model survives as the core: capcut-mate's segment-on-track model is structurally close but stateful/server-oriented; autocut's `CutList` is a specialized silence-removal representation, not a general timeline (audit §4). See `docs/project-format.md` for the actual schema.
- **`capcut/` is a direct Rust port** of `pyJianYingDraft` (Apache-2.0, permissively licensed, µs timebase already matches) — the highest-confidence reuse in the whole project (audit §3/§8).
- **`media/`/`vad/`/`audio/` are reimplementations** of autocut's algorithms in i64-µs (the user has obtained permission from autocut's author, Mert Cobanov, to reuse the code directly — see `docs/upstream.md` — but the timebase rewrite from f64-seconds to i64-µs is still a full rewrite of the internal representation, not a wrapper, per audit §5).
- **`jobs/` mediates every long-running operation** (proxy gen, transcription, silence analysis, rendering, model download) so the UI thread never blocks (master prompt §43); progress flows Rust → Tauri event → frontend store → UI.

## Timebase conversion boundaries

The only three places `i64` microseconds convert to something else:

| Boundary | Conversion | Module |
|---|---|---|
| FFmpeg | µs → seconds (`f64`, formatted `%.6f`) for `-ss`/`-t`/`inpoint`/`outpoint` | `ffmpeg/` command builder |
| FCPXML | µs → rational frame count (`num/den` per project fps, drop-frame aware) | `fcpxml/` |
| CapCut draft | none — `pyJianYingDraft` already speaks µs | `capcut/` |

All three conversions are implemented once, centrally, in each adapter module — never inlined ad hoc elsewhere (master prompt §67).

## Frontend/backend type contract

`specta`/`tauri-specta` generates `src/types/` TypeScript types directly from the Rust command signatures and `ProjectV1` struct. This replaces autocut's manual "keep `types.ts` in sync" convention (a maintenance risk flagged in audit §6 risk #10) — the IPC surface here is much larger (full timeline + AI + CapCut adapter + jobs) so hand-mirroring would drift quickly.

## Internationalization (i18n)

Master prompt §47: initial UI languages are English and Vietnamese, "do NOT hardcode every string directly in components," and the architecture must be "ready for more languages." Implemented as a small hand-rolled module rather than a third-party i18n library — the app's actual requirement (nested-key lookup, `{{param}}` interpolation, English fallback, a persisted locale) doesn't justify another dependency.

- **Locale catalogs**: `src/locales/en.json` and `src/locales/vi.json`. Each is a plain JSON object namespaced by component/area (`common`, `topBar`, `leftPanel`, `rightPanel`, `timelinePanel`, `mediaLibrary`, `videoPlayer`, …), with string leaves nested under those namespace keys — e.g. `mediaLibrary.importFiles`. Adding a language is adding one more file plus one entry in `SUPPORTED_LOCALES`; no component code changes.
- **`src/lib/i18n.svelte.ts`**: the runtime. A `Locale = "en" | "vi"` union, a Svelte 5 `$state`-backed `I18nStore` class (same house pattern as `stores/media.svelte.ts`), and three stable call sites every component uses instead of literal strings:
  - `t(key: string, params?: Record<string, string | number>): string` — dot-path lookup into the active locale's catalog (`t("mediaLibrary.importButton")`), `{{name}}`-style interpolation, and a fallback to the English catalog (then the raw key itself) when a key is missing from the active locale — so a missing Vietnamese string degrades to English rather than blank text.
  - `setLocale(locale: Locale): void` — switches the active locale and persists the choice.
  - `currentLocale(): Locale` — reactive read of the active locale (e.g. to drive a switcher's selected value).
- **Persistence**: currently `localStorage` under the key `aiVideoEditor.locale`, defaulting to `"en"` when nothing is stored (OS/browser locale detection was judged not worth the complexity yet). This lives behind an exported `LocalePersistence` interface (`load`/`save`) specifically so the eventual Settings phase (master prompt §46) can swap in a Rust-backed app setting without touching `t()`/`setLocale()` call sites anywhere in the app — components never talk to `localStorage` directly.
- **Language switcher**: a small English/Tiếng Việt `<select>` in `TopBar.svelte`, calling `setLocale()` on change. Language autonyms in the switcher itself (`English` / `Tiếng Việt`) are intentionally not translated, matching standard i18n UI convention.
- **Retrofit scope**: every hardcoded string in the Phase 2/3 components (`TopBar`, `LeftPanel`, `RightPanel`, `TimelinePanel`, `MediaLibrary`, `VideoPlayer` — button labels, tab names, the "X — Phase N" placeholder text, tooltips, status/error text, dropdown options) now routes through `t()`. Non-linguistic tokens (timeline track shorthand `V1`/`V2`/`A1`/`CC`, numeric aspect-ratio labels like `16:9`) are deliberately left as literals — they aren't language-dependent text.

## Related documents

- `docs/architecture-audit.md` — Phase 0 findings this design is based on.
- `docs/project-format.md` — `project.json` schema, timebase rationale, error model.
- `docs/upstream.md` — per-module provenance (ported / reimplemented / new).
- `docs/timeline.md`, `docs/render-engine.md`, `docs/capcut-integration.md`, `docs/ai-engine.md`, `docs/transcription.md` — written as their respective phases (4, 6, 9, 10, 7) are implemented, not upfront.
