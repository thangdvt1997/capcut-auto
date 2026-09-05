<!--
  Menu/toolbar strip (master-prompt §48). Edit/View/Help are still inert
  labels (no actions exist yet to wire them to — undo/redo already live as
  real Timeline toolbar buttons, not here). **File is now a real dropdown**
  (Phase 6 pass): "Export…" opens the Export/Render dialog
  (`stores/render.svelte.ts` / `ExportDialog.svelte`, mounted once in
  `App.svelte`) and "Export to FCPXML…" is the one-line FCPXML save action
  the Phase 6 task brief allowed as in-scope (full CapCut-adapter-flavored
  export UI is Phase 9's job, not this one). This is the "File > Export…"
  placement half of Phase 6's two-entry-point decision — the other entry
  point is a toolbar button in `Timeline.svelte`, matching Phase 5's
  Silence Detector precedent (see that component's own doc comment).
  The two other real, working pieces are:
    - the "New Project" button, which calls the actual `new_project` Rust
      command through the specta-generated bindings and proves the
      ProjectV1 schema round-trips over IPC;
    - the status chip, which calls `get_shell_info` and proves the
      specta -> TypeScript pipeline end-to-end.

  Phase 7 pass: added a "Models…" button, this app's chosen entry point for
  the Model Manager dialog (`ModelManagerDialog.svelte` / `stores/
  modelManager.svelte.ts`) in the absence of a built master-prompt §46
  Settings surface — see that component's doc comment for the full
  placement rationale.

  Phase 9 pass: added "Export to CapCut…" to the File dropdown (opens
  `CapCutExportDialog.svelte` / `stores/capcut.svelte.ts`, mounted once in
  `App.svelte`, alongside "Export…"/"Export to FCPXML…") and a "CapCut…"
  button next to "Models…" (opens `CapCutSettingsDialog.svelte`, same
  no-Settings-surface-yet placement rationale as "Models…" above).

  Phase 10 pass: added an "AI Settings…" button next to "CapCut…" (opens
  `AiSettingsDialog.svelte` / `stores/aiSettings.svelte.ts`, same
  no-Settings-surface-yet placement rationale as "Models…"/"CapCut…" above —
  see that dialog's own doc comment).
-->
<script lang="ts">
  import { commands } from "../../types/bindings";
  import type { ShellInfo, ProjectV1 } from "../../types/bindings";
  import { t, currentLocale, setLocale, type Locale } from "../../lib/i18n.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { renderStore } from "../../stores/render.svelte";
  import { modelManagerStore } from "../../stores/modelManager.svelte";
  import { capcutStore } from "../../stores/capcut.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";

  let shellInfo: ShellInfo | null = $state(null);
  let shellInfoError: string | null = $state(null);
  let lastProject: ProjectV1 | null = $state(null);
  let projectError: string | null = $state(null);
  let fileMenuOpen = $state(false);

  $effect(() => {
    commands
      .getShellInfo()
      .then((info) => {
        shellInfo = info;
      })
      .catch((err: unknown) => {
        shellInfoError = String(err);
      });
  });

  async function createProject() {
    projectError = null;
    try {
      lastProject = await commands.newProject("Untitled Project");
      // Feeds the fresh project into the Phase 4 timeline session
      // (`load_timeline_project`) so the Timeline panel has something real
      // to render — this button was previously Phase 2-only proof that the
      // IPC round trip works, and didn't touch the timeline at all.
      await timeline.loadProject(lastProject);
    } catch (err) {
      projectError = String(err);
    }
  }

  const inertMenuKeys = ["topBar.menuEdit", "topBar.menuView", "topBar.menuHelp"];

  function onLocaleChange(e: Event) {
    setLocale((e.currentTarget as HTMLSelectElement).value as Locale);
  }

  function onFileMenuKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      fileMenuOpen = false;
    }
  }
</script>

<header class="topbar">
  <span class="brand">
    <span class="brand-dot"></span>
    {t("common.appName")}
  </span>

  <div class="file-menu">
    <button
      class="menu-item menu-item-button"
      aria-haspopup="true"
      aria-expanded={fileMenuOpen}
      onclick={() => (fileMenuOpen = !fileMenuOpen)}
    >
      {t("topBar.menuFile")}
    </button>
    {#if fileMenuOpen}
      <div class="file-menu-backdrop" role="presentation" onclick={() => (fileMenuOpen = false)}></div>
      <div class="file-menu-dropdown" role="menu" tabindex="-1" onkeydown={onFileMenuKeydown}>
        <button
          class="file-menu-option"
          role="menuitem"
          onclick={() => {
            fileMenuOpen = false;
            renderStore.openDialog();
          }}
        >
          {t("topBar.menuExport")}
        </button>
        <button
          class="file-menu-option"
          role="menuitem"
          disabled={!timeline.project || renderStore.fcpxmlExporting}
          onclick={() => {
            fileMenuOpen = false;
            void renderStore.exportFcpxml();
          }}
        >
          {renderStore.fcpxmlExporting ? t("topBar.menuExportFcpxmlBusy") : t("topBar.menuExportFcpxml")}
        </button>
        <button
          class="file-menu-option"
          role="menuitem"
          disabled={!timeline.project}
          onclick={() => {
            fileMenuOpen = false;
            capcutStore.openExport();
          }}
        >
          {t("topBar.menuExportCapcut")}
        </button>
      </div>
    {/if}
  </div>

  {#each inertMenuKeys as key (key)}
    <span class="menu-item">{t(key)}</span>
  {/each}

  <button class="btn btn-ghost" onclick={createProject} title={t("topBar.newProjectTooltip")}>
    {t("topBar.newProjectButton")}
  </button>

  <!-- Phase 7 Model Manager entry point (see ModelManagerDialog.svelte's doc
       comment for the full placement rationale: no Settings surface exists
       yet to host this as a section, so it's a standalone dialog reachable
       from here). -->
  <button
    class="btn btn-ghost"
    onclick={() => modelManagerStore.openDialog()}
    title={t("topBar.modelManagerTooltip")}
  >
    {t("topBar.modelManagerButton")}
  </button>

  <!-- Phase 9 CapCut/Jianying Settings entry point (master prompt §30) —
       same placement rationale as the "Models…" button just above: no
       master prompt §46 Settings surface exists yet to host this as a
       section, so it's a standalone dialog reachable from here (see
       CapCutSettingsDialog.svelte's own doc comment). -->
  <button
    class="btn btn-ghost"
    onclick={() => capcutStore.openSettings()}
    title={t("topBar.capcutSettingsTooltip")}
  >
    {t("topBar.capcutSettingsButton")}
  </button>

  <!-- Phase 10 AI Settings entry point (master prompt §17) — same
       placement rationale as "Models…"/"CapCut…" just above: no master
       prompt §46 Settings surface exists yet to host this as a section, so
       it's a standalone dialog reachable from here (see
       AiSettingsDialog.svelte's own doc comment). -->
  <button
    class="btn btn-ghost"
    onclick={() => aiSettingsStore.openDialog()}
    title={t("topBar.aiSettingsTooltip")}
  >
    {t("topBar.aiSettingsButton")}
  </button>

  {#if lastProject}
    <span class="status-chip">{t("topBar.projectStatus", { name: lastProject.project.name, id: lastProject.project.id.slice(0, 8) })}</span>
  {:else if projectError}
    <span class="status-chip" style:color="var(--neg)">{t("topBar.newProjectFailed", { error: projectError })}</span>
  {/if}

  {#if renderStore.fcpxmlError}
    <span class="status-chip" style:color="var(--neg)">{t("topBar.exportFcpxmlFailed", { error: renderStore.fcpxmlError })}</span>
  {:else if renderStore.fcpxmlLastPath}
    <span class="status-chip">{t("topBar.exportFcpxmlDone")}</span>
  {/if}

  <span class="topbar-spacer"></span>

  {#if shellInfo}
    <span class="status-chip">
      {t("topBar.shellInfoStatus", {
        version: shellInfo.app_version,
        tauriVersion: shellInfo.tauri_version,
        os: shellInfo.os,
        arch: shellInfo.arch,
      })}
    </span>
  {:else if shellInfoError}
    <span class="status-chip" style:color="var(--neg)">{t("topBar.shellInfoUnavailable", { error: shellInfoError })}</span>
  {:else}
    <span class="status-chip muted-2">{t("topBar.loading")}</span>
  {/if}

  <select
    class="lang-select"
    aria-label={t("common.languageLabel")}
    title={t("common.languageLabel")}
    value={currentLocale()}
    onchange={onLocaleChange}
  >
    <option value="en">English</option>
    <option value="vi">Tiếng Việt</option>
  </select>
</header>

<style>
  .lang-select {
    height: 26px;
    margin-left: 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11px;
  }
  .file-menu {
    position: relative;
  }
  .menu-item-button {
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .menu-item-button[aria-expanded="true"] {
    color: var(--foreground);
  }
  .file-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .file-menu-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 6px;
    min-width: 190px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 10px 30px hsl(0 0% 0% / 0.4);
    z-index: 100;
    overflow: hidden;
  }
  .file-menu-option {
    text-align: left;
    padding: 8px 12px;
    background: none;
    border: none;
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .file-menu-option:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .file-menu-option:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
