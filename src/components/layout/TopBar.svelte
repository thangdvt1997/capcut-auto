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
    } catch (err) {
      projectError = String(err);
    }
  }

  const menus = ["File", "Edit", "View", "Help"];
</script>

<header class="topbar">
  <span class="brand">
    <span class="brand-dot"></span>
    AI Video Editor
  </span>

  {#each menus as menu (menu)}
    <span class="menu-item">{menu}</span>
  {/each}

  <button class="btn btn-ghost" onclick={createProject} title="Constructs a fresh ProjectV1 in memory (not yet saved to disk — Project Manager persistence is a later phase)">
    New Project
  </button>

  {#if lastProject}
    <span class="status-chip">project: {lastProject.project.name} ({lastProject.project.id.slice(0, 8)})</span>
  {:else if projectError}
    <span class="status-chip" style:color="var(--neg)">new_project failed: {projectError}</span>
  {/if}

  <span class="topbar-spacer"></span>

  {#if shellInfo}
    <span class="status-chip">
      v{shellInfo.app_version} · tauri {shellInfo.tauri_version} · {shellInfo.os}/{shellInfo.arch}
    </span>
  {:else if shellInfoError}
    <span class="status-chip" style:color="var(--neg)">shell info unavailable: {shellInfoError}</span>
  {:else}
    <span class="status-chip muted-2">loading…</span>
  {/if}
</header>
