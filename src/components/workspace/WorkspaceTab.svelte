<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace") root. Owns the
  Simple/Advanced mode toggle — the resolution to this task's own
  "does Tab 1 replace or wrap the existing Timeline NLE" question (see
  `STUDIO_PLAN.md`'s "UI Redesign Audit" §5 for the original blocking
  question, now decided here):

  **Decision: a mode toggle within Tab 1 itself, not a 4th top-level tab.**
  promt.md §2 asks for exactly 3 top-level tabs, and Tab 1 IS "the video
  workspace" either way — switching between a simple pipeline view and the
  full editor for that same workspace is a coherent "same task, different
  density" pattern (not a different task deserving its own tab), and it's
  the cheapest option that keeps promt.md's own literal §28 mockup as Tab
  1's real, un-buried default identity. "Simple" (promt.md's own mockup:
  `WorkspaceSimple.svelte`) is the default on first load; "Advanced" (the
  pre-existing full multi-track editor: `WorkspaceAdvanced.svelte`,
  unchanged) is one click away via the toggle immediately below TopBar,
  every time Tab 1 is open — not buried in a menu, not removed. The choice
  persists per-browser-profile in `localStorage` (same "remember a UI
  preference, no backend setting exists for this" precedent every other
  `stores/*.svelte.ts` preference in this app already uses).

  Reuses the real `Tabs` Design System component (Phase D1) for the toggle
  itself rather than hand-rolling a second tab-strip implementation.
-->
<script lang="ts">
  import Tabs from "../ui/Tabs.svelte";
  import WorkspaceSimple from "./WorkspaceSimple.svelte";
  import WorkspaceAdvanced from "../layout/WorkspaceAdvanced.svelte";
  import { t } from "../../lib/i18n.svelte";

  const STORAGE_KEY = "ave:workspace:mode";

  function loadMode(): string {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      return raw === "advanced" ? "advanced" : "simple";
    } catch {
      return "simple";
    }
  }

  let mode = $state<string>(loadMode());

  function onModeChange(next: string): void {
    mode = next;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* storage may be disabled — the choice simply won't survive a restart */
    }
  }
</script>

<div class="workspace-tab">
  <div class="mode-bar">
    <span class="mode-label muted-2">{t("workspaceTab.modeLabel")}</span>
    <Tabs
      tabs={[
        { id: "simple", label: t("workspaceTab.modeSimple") },
        { id: "advanced", label: t("workspaceTab.modeAdvanced") },
      ]}
      active={mode}
      onChange={onModeChange}
    />
    <span class="mode-desc muted-2">{t("workspaceTab.modeDescription")}</span>
  </div>

  <div class="workspace-body">
    {#if mode === "advanced"}
      <WorkspaceAdvanced />
    {:else}
      <WorkspaceSimple />
    {/if}
  </div>
</div>

<style>
  .workspace-tab {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .mode-bar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3) 0;
  }
  .mode-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .mode-desc {
    font-size: 10.5px;
  }
  .workspace-body {
    flex: 1;
    min-height: 0;
  }
</style>
