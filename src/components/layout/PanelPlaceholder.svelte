<!--
  Generic "not built yet" panel body. Every workspace panel in Phase 2 is
  empty by design (see IMPLEMENTATION_PLAN.md Phase 2) — this component
  keeps that fact visible in the UI instead of silently rendering nothing,
  per the master prompt §75 "do not fake features" rule.
-->
<script lang="ts">
  type Props = {
    title: string;
    phase: string;
  };
  let { title, phase }: Props = $props();
</script>

<div class="panel-placeholder">
  <div>
    <div class="ph-title">{title}</div>
    <div class="ph-phase">{phase}</div>
  </div>
</div>
