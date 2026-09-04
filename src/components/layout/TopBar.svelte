<!--
  Menu/toolbar strip (master-prompt §48). The File/Edit/View/Help entries
  are inert labels in Phase 2 (no menu actions exist yet to wire them to —
  Project Manager, undo/redo etc. land in later phases). The two real,
  working pieces are:
    - the "New Project" button, which calls the actual `new_project` Rust
      command through the specta-generated bindings and proves the
      ProjectV1 schema round-trips over IPC;
    - the status chip, which calls `get_shell_info` and proves the
      specta -> TypeScript pipeline end-to-end.
-->
<script lang="ts">
  import { commands } from "../../types/bindings";
  import type { ShellInfo, ProjectV1 } from "../../types/bindings";
  import { t, currentLocale, setLocale, type Locale } from "../../lib/i18n.svelte";
  import { timeline } from "../../stores/timeline.svelte";

  let shellInfo: ShellInfo | null = $state(null);
  let shellInfoError: string | null = $state(null);
  let lastProject: ProjectV1 | null = $state(null);
  let projectError: string | null = $state(null);

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

  const menuKeys = ["topBar.menuFile", "topBar.menuEdit", "topBar.menuView", "topBar.menuHelp"];

  function onLocaleChange(e: Event) {
    setLocale((e.currentTarget as HTMLSelectElement).value as Locale);
  }
</script>

<header class="topbar">
  <span class="brand">
    <span class="brand-dot"></span>
    {t("common.appName")}
  </span>

  {#each menuKeys as key (key)}
    <span class="menu-item">{t(key)}</span>
  {/each}

  <button class="btn btn-ghost" onclick={createProject} title={t("topBar.newProjectTooltip")}>
    {t("topBar.newProjectButton")}
  </button>

  {#if lastProject}
    <span class="status-chip">{t("topBar.projectStatus", { name: lastProject.project.name, id: lastProject.project.id.slice(0, 8) })}</span>
  {:else if projectError}
    <span class="status-chip" style:color="var(--neg)">{t("topBar.newProjectFailed", { error: projectError })}</span>
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
</style>
