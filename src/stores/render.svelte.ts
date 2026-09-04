// Svelte 5 runes-based store for the Phase 6 Export/Render dialog (master
// prompt §32/§33/§43/§44). Composes with `stores/timeline.svelte.ts` the same
// way `stores/silenceDetector.svelte.ts` does (a distinct, self-contained
// workflow with its own transient state), rather than living inside it.
//
// Workflow: pick a preset (seeds the settings form) -> optionally override
// individual fields -> pick an output path -> Export -> progress via the
// `render:progress` Tauri event, keyed by `job_id` (not assumed singular,
// per the task brief, even though this dialog only ever starts one job at a
// time) -> Cancel while running, or a completed/error result once done.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type {
  AudioCodec,
  Container,
  DetectedEncoder,
  EncoderBackend,
  HardwareEncoderReport,
  ProjectV1,
  Rational,
  RenderPreset,
  RenderSettings,
  RenderSettingsInput,
  VideoCodec,
} from "../types/bindings";
import { timeline } from "./timeline.svelte";

/**
 * Plain-object copy of a Svelte 5 `$state` reactive value, safe to hand to
 * an IPC call. Same trick as `stores/timeline.svelte.ts`'s own `snap()`
 * helper (a generic wrapper sidesteps TS's recursive `Snapshot<T>` mapped
 * type blowing up on `ProjectV1`'s recursive `JsonValue` fields) — not
 * exported from there, so duplicated here rather than reaching into that
 * module's internals.
 */
function snap<T>(value: T): T {
  return $state.snapshot(value) as T;
}

/**
 * Payload of the `render:progress` Tauri event
 * (`src-tauri/src/commands/render.rs::RenderProgressEvent`). Hand-written
 * rather than specta-generated, matching `stores/media.svelte.ts`'s own
 * `ProxyProgressEvent` precedent — this `tauri-specta` `Builder` only
 * registers *commands*, not typed events (see that file's doc comment for
 * the full rationale). Keep in sync with the Rust struct by hand.
 */
export interface RenderProgressEvent {
  job_id: string;
  fraction: number | null;
  speed: number | null;
  done: boolean;
  output_path: string | null;
  error: string | null;
}

const RENDER_PROGRESS_EVENT = "render:progress";

/** CRF ("quality") vs. explicit bitrate are mutually exclusive concepts in
 * `RenderSettings` (`crf: Option<u8>` / `video_bitrate_kbps: Option<u32>` —
 * exactly one is set); this mirrors that as a two-way UI toggle rather than
 * exposing both number inputs live at once. */
export type BitrateMode = "crf" | "bitrate";

const COMMON_FPS: { label: string; value: Rational }[] = [
  { label: "23.976", value: { num: 24000, den: 1001 } },
  { label: "24", value: { num: 24, den: 1 } },
  { label: "25", value: { num: 25, den: 1 } },
  { label: "29.97", value: { num: 30000, den: 1001 } },
  { label: "30", value: { num: 30, den: 1 } },
  { label: "50", value: { num: 50, den: 1 } },
  { label: "59.94", value: { num: 60000, den: 1001 } },
  { label: "60", value: { num: 60, den: 1 } },
];

const X264_PRESETS = [
  "ultrafast",
  "superfast",
  "veryfast",
  "faster",
  "fast",
  "medium",
  "slow",
  "slower",
  "veryslow",
];

function fpsKey(fps: Rational): string {
  return `${fps.num}/${fps.den}`;
}

class RenderStore {
  open = $state(false);

  // ---- Presets ----------------------------------------------------------

  presets = $state<RenderPreset[]>([]);
  presetsLoading = $state(false);
  presetsError = $state<string | null>(null);
  selectedPresetId = $state<string | null>(null);

  // ---- Hardware acceleration (master prompt §33) -------------------------

  hardware = $state<HardwareEncoderReport | null>(null);
  hardwareLoading = $state(false);
  hardwareError = $state<string | null>(null);

  // ---- Settings form (mirrors `RenderSettings`, all independently
  // overridable — a preset only *seeds* these, per master prompt §32) -----

  width = $state(1920);
  height = $state(1080);
  fps = $state<Rational>({ num: 30, den: 1 });
  container = $state<Container>("mp_4");
  videoCodec = $state<VideoCodec>("h264");
  x264Preset = $state("medium");
  bitrateMode = $state<BitrateMode>("crf");
  crf = $state(20);
  videoBitrateKbps = $state(8000);
  audioCodec = $state<AudioCodec>("aac");
  audioBitrateKbps = $state(192);
  /** `null` = auto-detect (master prompt §33's "do capability detection
   * rather than assuming hardware exists"). */
  hardwareEncoder = $state<EncoderBackend | null>(null);

  outputPath = $state<string | null>(null);

  // ---- Job lifecycle ------------------------------------------------------

  jobId = $state<string | null>(null);
  /** Keyed by `job_id`, not just "the current job" — a project can only
   * sensibly render one job from this dialog, but the store itself doesn't
   * assume that (task brief), so a stale event for a since-superseded job
   * can never clobber the live one's progress. */
  progressByJob = $state<Record<string, RenderProgressEvent>>({});
  starting = $state(false);
  cancelling = $state(false);
  startError = $state<string | null>(null);

  // ---- FCPXML export (master prompt §31 defers the full CapCut-flavored
  // UI to Phase 9; this is just the small, settings-free "save a .fcpxml
  // file" action the task brief allowed as an in-scope one-liner) ---------

  fcpxmlExporting = $state(false);
  fcpxmlError = $state<string | null>(null);
  fcpxmlLastPath = $state<string | null>(null);

  constructor() {
    // Fire-and-forget, matching `stores/media.svelte.ts`'s
    // `ProxyProgressEvent` listener pattern exactly.
    void listen<RenderProgressEvent>(RENDER_PROGRESS_EVENT, (event) => {
      this.progressByJob[event.payload.job_id] = event.payload;
    });
  }

  // -------------------------------------------------------------------
  // Derived
  // -------------------------------------------------------------------

  progress = $derived(this.jobId ? (this.progressByJob[this.jobId] ?? null) : null);
  isRendering = $derived(this.jobId !== null && !(this.progress?.done ?? false));

  /** Container gates which codec *options* are offered (master prompt §32:
   * MP4 -> H.264/H.265, WebM -> VP9) — kept in sync with
   * `RenderSettings::validate()`'s pairing rules in
   * `src-tauri/src/render/presets.rs`, not re-derived from it (there's no
   * shared codegen for validation logic, only for types). */
  videoCodecOptions = $derived<VideoCodec[]>(this.container === "mp_4" ? ["h264", "h265"] : ["vp_9"]);
  audioCodecOptions = $derived<AudioCodec[]>(this.container === "mp_4" ? ["aac"] : ["opus", "vorbis"]);

  fpsOptions = $derived(COMMON_FPS);
  /** The current `fps` might not be one of `COMMON_FPS` (e.g. a preset ever
   * introduces an odd rate) — falls back to a synthesized "custom" entry so
   * the dropdown always has a matching option instead of silently
   * displaying the wrong value. */
  fpsSelectOptions = $derived.by((): { label: string; key: string }[] => {
    const known = COMMON_FPS.map((f) => ({ label: f.label, key: fpsKey(f.value) }));
    const currentKey = fpsKey(this.fps);
    if (known.some((f) => f.key === currentKey)) return known;
    return [...known, { label: `${currentKey} (custom)`, key: currentKey }];
  });
  fpsSelectValue = $derived(fpsKey(this.fps));

  detectedWorkingEncoders = $derived<DetectedEncoder[]>(
    (this.hardware?.encoders ?? []).filter((e) => e.working),
  );

  canExport = $derived(
    timeline.project !== null && this.outputPath !== null && !this.starting && !this.isRendering,
  );

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  /** Opens the dialog; lazily loads presets/hardware info on first open
   * rather than eagerly at app startup (task brief point 3: "on dialog
   * open, or lazily on first relevant interaction"). */
  openDialog(): void {
    this.open = true;
    void this.ensurePresetsLoaded();
    void this.ensureHardwareDetected();
  }

  /** Deliberately does NOT reset job/progress state — closing the dialog
   * while a render is in flight doesn't cancel it (the backend keeps
   * running regardless, per master prompt §43), and reopening should still
   * show where that job stands. Only an explicit Cancel stops a job. */
  close(): void {
    this.open = false;
  }

  async ensurePresetsLoaded(): Promise<void> {
    if (this.presets.length > 0 || this.presetsLoading) return;
    this.presetsLoading = true;
    this.presetsError = null;
    try {
      this.presets = await commands.listRenderPresets();
      const [firstPreset] = this.presets;
      if (this.selectedPresetId === null && firstPreset) {
        const default1080 = this.presets.find((p) => p.id === "p1080");
        this.selectPreset((default1080 ?? firstPreset).id);
      }
    } catch (err) {
      this.presetsError = String(err);
    } finally {
      this.presetsLoading = false;
    }
  }

  async ensureHardwareDetected(): Promise<void> {
    if (this.hardware !== null || this.hardwareLoading) return;
    this.hardwareLoading = true;
    this.hardwareError = null;
    try {
      const result = await commands.detectHardwareEncoders();
      if (result.status === "ok") {
        this.hardware = result.data;
      } else {
        this.hardwareError = result.error.message;
      }
    } finally {
      this.hardwareLoading = false;
    }
  }

  // -------------------------------------------------------------------
  // Settings form
  // -------------------------------------------------------------------

  /** Selecting a preset seeds every field below from it (master prompt
   * §32: presets are starting points, not exclusive of manual control) —
   * the user can still override any individual field afterward. */
  selectPreset(id: string): void {
    const preset = this.presets.find((p) => p.id === id);
    if (!preset) return;
    this.selectedPresetId = id;
    this.applySettings(preset.settings);
  }

  private applySettings(s: RenderSettings): void {
    this.width = s.width;
    this.height = s.height;
    this.fps = s.fps;
    this.container = s.container;
    this.videoCodec = s.video_codec;
    this.x264Preset = s.x264_preset;
    this.bitrateMode = s.crf !== null ? "crf" : "bitrate";
    if (s.crf !== null) this.crf = s.crf;
    if (s.video_bitrate_kbps !== null) this.videoBitrateKbps = s.video_bitrate_kbps;
    this.audioCodec = s.audio_codec;
    this.audioBitrateKbps = s.audio_bitrate_kbps;
    this.hardwareEncoder = s.hardware_encoder;
  }

  /** Changing the container keeps the codec dropdowns' *options* consistent
   * with it (task brief point 2) rather than letting the user pick an
   * impossible combo the backend's `RenderSettings::validate()` would only
   * reject after clicking Export. */
  setContainer(next: Container): void {
    this.container = next;
    const videoCodecs: VideoCodec[] = next === "mp_4" ? ["h264", "h265"] : ["vp_9"];
    if (!videoCodecs.includes(this.videoCodec)) this.videoCodec = next === "mp_4" ? "h264" : "vp_9";
    const audioCodecs: AudioCodec[] = next === "mp_4" ? ["aac"] : ["opus", "vorbis"];
    if (!audioCodecs.includes(this.audioCodec)) this.audioCodec = next === "mp_4" ? "aac" : "opus";
  }

  setFpsByKey(key: string): void {
    const match = COMMON_FPS.find((f) => fpsKey(f.value) === key);
    if (match) this.fps = match.value;
  }

  /** What's actually sent to `start_render_job`. Every field the form
   * tracks is sent explicitly (not just the ones the user touched) — the
   * backend's `resolve_settings` layers overrides on top of `preset_id`'s
   * base settings field-by-field, and since every field here already holds
   * either the preset's own value or the user's override, sending all of
   * them resolves to the same settings either way. Simpler than tracking
   * "which fields did the user actually touch" for no behavioral gain. */
  buildSettingsInput(): RenderSettingsInput {
    return {
      preset_id: this.selectedPresetId,
      width: this.width,
      height: this.height,
      fps: this.fps,
      container: this.container,
      video_codec: this.videoCodec,
      x264_preset: this.x264Preset,
      crf: this.bitrateMode === "crf" ? this.crf : null,
      video_bitrate_kbps: this.bitrateMode === "bitrate" ? this.videoBitrateKbps : null,
      audio_codec: this.audioCodec,
      audio_bitrate_kbps: this.audioBitrateKbps,
      hardware_encoder: this.hardwareEncoder,
    };
  }

  // -------------------------------------------------------------------
  // Output path (save() dialog, not open() — master prompt task brief
  // point 4 — filtered to the chosen container's extension)
  // -------------------------------------------------------------------

  async chooseOutputPath(): Promise<void> {
    const ext = this.container === "mp_4" ? "mp4" : "webm";
    const chosen = await save({
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      defaultPath: `export.${ext}`,
    });
    if (chosen) this.outputPath = chosen;
  }

  // -------------------------------------------------------------------
  // Start / cancel (master prompt §43/§44 — progress flows Rust -> Tauri
  // event -> this store -> UI; cancellation wires to a real backend
  // process-kill, no client-side fakery)
  // -------------------------------------------------------------------

  async startExport(): Promise<void> {
    const project: ProjectV1 | null = timeline.project;
    if (!project || !this.outputPath || this.starting || this.isRendering) return;
    this.starting = true;
    this.startError = null;
    try {
      const result = await commands.startRenderJob(snap(project), this.buildSettingsInput(), this.outputPath);
      if (result.status === "ok") {
        this.jobId = result.data;
        this.progressByJob[result.data] = {
          job_id: result.data,
          fraction: 0,
          speed: null,
          done: false,
          output_path: null,
          error: null,
        };
      } else {
        this.startError = result.error.message;
      }
    } finally {
      this.starting = false;
    }
  }

  async cancel(): Promise<void> {
    if (!this.jobId || this.cancelling) return;
    this.cancelling = true;
    try {
      const result = await commands.cancelRenderJob(this.jobId);
      if (result.status === "error") {
        this.startError = result.error.message;
      }
    } finally {
      this.cancelling = false;
    }
  }

  /** Clears the finished job's result so the form is usable for a fresh
   * export without closing/reopening the dialog. */
  startNewExport(): void {
    this.jobId = null;
    this.startError = null;
  }

  // -------------------------------------------------------------------
  // FCPXML export (small, settings-free — see class doc comment)
  // -------------------------------------------------------------------

  async exportFcpxml(): Promise<void> {
    const project = timeline.project;
    if (!project || this.fcpxmlExporting) return;
    const chosen = await save({
      filters: [{ name: "FCPXML", extensions: ["fcpxml"] }],
      defaultPath: "export.fcpxml",
    });
    if (!chosen) return;
    this.fcpxmlExporting = true;
    this.fcpxmlError = null;
    try {
      const result = await commands.exportFcpxml(snap(project), chosen);
      if (result.status === "error") {
        this.fcpxmlError = result.error.message;
      } else {
        this.fcpxmlLastPath = chosen;
      }
    } finally {
      this.fcpxmlExporting = false;
    }
  }
}

export const renderStore = new RenderStore();
export { X264_PRESETS };
