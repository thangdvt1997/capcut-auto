<!--
  Design System gap-fill (Phase D9, `STUDIO_PLAN.md`): the real right-click
  context menu deferred since Phase D1 itself ("Deferred: ContextMenu — not
  built... Real, honest gap — not silently dropped"). Confirmed via a
  repo-wide grep immediately before building this (`grep -rn
  "contextmenu|oncontextmenu" src/`) that nothing in this app currently wires
  up a right-click handler anywhere — so this is built proactively, per
  Phase D1's own original scope, and is NOT yet consumed by any dialog. A
  future pass that wants a real right-click menu (e.g. Timeline clips, Media
  Library items, Asset Library rows) has a solid, real component to wire up
  instead of hand-rolling one from scratch.

  Controlled component, mirroring `Modal.svelte`'s own `open`/`onClose`
  convention: the caller owns `open`/`x`/`y` (typically set from its own
  `oncontextmenu` handler via `e.preventDefault()` + `e.clientX`/`e.clientY`)
  and this component only renders/positions/closes. A transparent full-screen
  backdrop (same technique as `Modal.svelte`'s dimmed one, just not tinted)
  catches an outside click or a second right-click to close; Escape closes
  too. Position is clamped to the viewport after mount so the menu never
  renders partially off-screen near an edge/corner.

  Honest v1 scope (matching `DataTable.svelte`'s own "real, honestly-scoped
  v1" precedent from Phase D1): no submenu nesting, no arrow-key roving
  `tabindex` (a mouse-driven menu today — Escape-to-close and a disabled
  state are the only keyboard/a11y affordances built so far), no "flip to
  the other side" logic beyond simple edge clamping.
-->
<script module lang="ts">
  export type ContextMenuItem = {
    id: string;
    label: string;
    icon?: string;
    danger?: boolean;
    disabled?: boolean;
    onSelect: () => void;
  };
</script>

<script lang="ts">
  let {
    open,
    x,
    y,
    items,
    ariaLabel,
    onClose,
  }: {
    open: boolean;
    x: number;
    y: number;
    items: ContextMenuItem[];
    ariaLabel: string;
    onClose: () => void;
  } = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    if (!open || !menuEl) return;
    const margin = 4;
    const rect = menuEl.getBoundingClientRect();
    const maxLeft = Math.max(margin, window.innerWidth - rect.width - margin);
    const maxTop = Math.max(margin, window.innerHeight - rect.height - margin);
    pos = {
      left: Math.max(margin, Math.min(x, maxLeft)),
      top: Math.max(margin, Math.min(y, maxTop)),
    };
  });

  function handleSelect(item: ContextMenuItem): void {
    if (item.disabled) return;
    item.onSelect();
    onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  function onBackdropContextmenu(e: MouseEvent): void {
    // A second right-click (e.g. somewhere else while this menu is already
    // open) closes it instead of leaving it open underneath a new one.
    e.preventDefault();
    onClose();
  }
</script>

{#if open}
  <div
    class="ui-context-menu-backdrop"
    role="presentation"
    onclick={onClose}
    oncontextmenu={onBackdropContextmenu}
  >
    <div
      bind:this={menuEl}
      class="ui-context-menu"
      style="left: {pos.left}px; top: {pos.top}px;"
      role="menu"
      aria-label={ariaLabel}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      {#each items as item (item.id)}
        <button
          type="button"
          class="ui-context-menu-item"
          class:ui-context-menu-item-danger={item.danger}
          role="menuitem"
          disabled={item.disabled}
          onclick={() => handleSelect(item)}
        >
          {#if item.icon}<span class="ui-context-menu-icon" aria-hidden="true">{item.icon}</span>{/if}
          <span class="ui-context-menu-label">{item.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .ui-context-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
  }
  .ui-context-menu {
    position: fixed;
    display: flex;
    flex-direction: column;
    min-width: 160px;
    max-width: 280px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px hsl(0 0% 0% / 0.45);
  }
  .ui-context-menu-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    cursor: pointer;
  }
  .ui-context-menu-item:hover:not(:disabled) {
    background: var(--elevated);
  }
  .ui-context-menu-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .ui-context-menu-item-danger {
    color: var(--neg);
  }
  .ui-context-menu-icon {
    flex-shrink: 0;
    width: 14px;
    text-align: center;
  }
  .ui-context-menu-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
