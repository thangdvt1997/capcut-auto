<!--
  THROWAWAY verification aid, not a shipped feature (Phase D1 task brief
  explicitly allows this). Not imported anywhere in the live app — exercises
  every ui/ component's real props/types in one place as a compile-time +
  visual sanity check. Safe to delete at any time; kept only as a live
  reference for whoever does the Phase D2+ retrofit.
-->
<script lang="ts">
  import Button from "../Button.svelte";
  import IconButton from "../IconButton.svelte";
  import Badge from "../Badge.svelte";
  import ProgressBar from "../ProgressBar.svelte";
  import Tooltip from "../Tooltip.svelte";
  import EmptyState from "../EmptyState.svelte";
  import LoadingState from "../LoadingState.svelte";
  import ErrorState from "../ErrorState.svelte";
  import Input from "../Input.svelte";
  import Select from "../Select.svelte";
  import Checkbox from "../Checkbox.svelte";
  import Switch from "../Switch.svelte";
  import Slider from "../Slider.svelte";
  import Card from "../Card.svelte";
  import Panel from "../Panel.svelte";
  import Modal from "../Modal.svelte";
  import Tabs from "../Tabs.svelte";
  import Toast from "../Toast.svelte";
  import DataTable from "../DataTable.svelte";
  import type { DataTableColumn } from "../DataTable.svelte";
  import { toastStore } from "../../../stores/toast.svelte";
  import NumberInput from "../NumberInput.svelte";
  import RadioGroup from "../RadioGroup.svelte";
  import SuccessBanner from "../SuccessBanner.svelte";
  import ContextMenu from "../ContextMenu.svelte";
  import type { ContextMenuItem } from "../ContextMenu.svelte";

  let inputValue = $state("hello");
  let selectValue = $state("b");
  let checked = $state(true);
  let switchOn = $state(false);
  let sliderValue = $state(35);
  let modalOpen = $state(false);
  let activeTab = $state("one");
  let numberValue = $state(1920);
  let radioValue = $state("crf");
  let contextMenuOpen = $state(false);
  let contextMenuPos = $state({ x: 0, y: 0 });
  const contextMenuItems: ContextMenuItem[] = [
    { id: "rename", label: "Rename", onSelect: () => {} },
    { id: "duplicate", label: "Duplicate", onSelect: () => {} },
    { id: "delete", label: "Delete", danger: true, onSelect: () => {} },
  ];

  function openDemoContextMenu(e: MouseEvent): void {
    e.preventDefault();
    contextMenuPos = { x: e.clientX, y: e.clientY };
    contextMenuOpen = true;
  }

  type Row = { id: string; name: string; score: number };
  const rows: Row[] = [
    { id: "1", name: "Alpha", score: 92 },
    { id: "2", name: "Beta", score: 41 },
    { id: "3", name: "Gamma", score: 77 },
  ];
  const columns: DataTableColumn<Row>[] = [
    { key: "name", label: "Name", sortable: true, accessor: (r) => r.name },
    { key: "score", label: "Score", sortable: true, align: "right", accessor: (r) => r.score },
  ];
</script>

<div class="demo-page">
  <Panel title="Buttons">
    <div class="demo-row">
      <Button variant="primary">Primary</Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="danger">Danger</Button>
      <Button variant="ghost">Ghost</Button>
      <Button loading>Loading</Button>
      <Button disabled>Disabled</Button>
      <IconButton ariaLabel="Close">×</IconButton>
    </div>
  </Panel>

  <Panel title="Badges + Progress">
    <div class="demo-row">
      <Badge variant="pos">Healthy</Badge>
      <Badge variant="neg">Failed</Badge>
      <Badge variant="warn">Warning</Badge>
      <Badge variant="accent">In use</Badge>
    </div>
    <ProgressBar value={0.6} label="60%" />
    <ProgressBar indeterminate />
  </Panel>

  <Panel title="Tooltip">
    <Tooltip text="This is a real tooltip">
      <Button variant="ghost">Hover me</Button>
    </Tooltip>
  </Panel>

  <Panel title="States">
    <EmptyState icon="📭" title="Nothing here yet" description="Try importing a file." />
    <LoadingState />
    <ErrorState message="Something broke" onRetry={() => {}} />
  </Panel>

  <Panel title="Form primitives">
    <Input bind:value={inputValue} label="Name" placeholder="Type here" />
    <Select
      bind:value={selectValue}
      label="Choice"
      options={[
        { value: "a", label: "A" },
        { value: "b", label: "B" },
      ]}
    />
    <Checkbox bind:checked>Enabled</Checkbox>
    <Switch bind:checked={switchOn} ariaLabel="Toggle" />
    <Slider bind:value={sliderValue} label="Volume" formatValue={(v) => `${v}%`} />
    <NumberInput bind:value={numberValue} label="Width" min={2} step={2} />
    <RadioGroup
      bind:value={radioValue}
      name="demo-radio"
      label="Quality mode"
      options={[
        { value: "crf", label: "Constant quality (CRF)" },
        { value: "bitrate", label: "Target bitrate" },
      ]}
    />
  </Panel>

  <Panel title="Success banner">
    <SuccessBanner message="Render complete: C:\output\video.mp4" />
  </Panel>

  <Panel title="Context menu">
    <div
      class="demo-context-target"
      oncontextmenu={openDemoContextMenu}
      role="group"
      aria-label="Right-click this area to open a demo context menu"
    >
      Right-click here
    </div>
    <ContextMenu
      open={contextMenuOpen}
      x={contextMenuPos.x}
      y={contextMenuPos.y}
      items={contextMenuItems}
      ariaLabel="Demo context menu"
      onClose={() => (contextMenuOpen = false)}
    />
  </Panel>

  <Panel title="Card">
    <Card>Static card content</Card>
    <Card interactive onclick={() => {}}>Interactive card</Card>
  </Panel>

  <Panel title="Tabs">
    <Tabs
      bind:active={activeTab}
      tabs={[
        { id: "one", label: "One" },
        { id: "two", label: "Two" },
      ]}
    />
  </Panel>

  <Panel title="DataTable">
    <DataTable {columns} {rows} rowKey={(r) => r.id} emptyMessage="No rows" />
  </Panel>

  <Panel title="Modal + Toast">
    <Button onclick={() => (modalOpen = true)}>Open modal</Button>
    <Button onclick={() => toastStore.success("Saved successfully")}>Show toast</Button>
    <Modal open={modalOpen} title="Demo modal" onClose={() => (modalOpen = false)}>
      <p>Modal body content.</p>
      {#snippet footer()}
        <Button variant="ghost" onclick={() => (modalOpen = false)}>Cancel</Button>
        <Button variant="primary" onclick={() => (modalOpen = false)}>Confirm</Button>
      {/snippet}
    </Modal>
    <Toast />
  </Panel>
</div>

<style>
  .demo-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    padding: var(--space-4);
    max-width: 480px;
  }
  .demo-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }
  .demo-context-target {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 60px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--muted);
    font-size: 11.5px;
    user-select: none;
  }
</style>
