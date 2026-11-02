<script lang="ts">
  import { formatCount } from "./format";
  import type { Drives } from "./drives.svelte";
  import type { Volume } from "./api";

  let {
    drives,
    selected = $bindable(),
    folder = $bindable(),
    onManage,
  }: {
    drives: Drives;
    selected: number | null;
    folder: string | null;
    onManage: () => void;
  } = $props();

  const NEARLY_FULL = 0.9;

  const total = $derived(drives.catalogued.reduce((n, v) => n + v.fileCount, 0));

  function pick(id: number | null) {
    selected = id;
    folder = null;
  }

  const used = (v: Volume) => (v.totalBytes ? (v.totalBytes - v.freeBytes) / v.totalBytes : 0);
  const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 0 });
  /** Drives without a volume label are named after their mount path: show "Drive F:". */
  const nameOf = (v: Volume) => (/^[A-Za-z]:\\?$/.test(v.label) ? `Drive ${v.label.slice(0, 2)}` : v.label);
</script>

<div class="strip" role="group" aria-label="Drives">
  <button class="drive" class:on={selected === null} onclick={() => pick(null)}>
    <span class="line">
      <span class="dot on"></span>
      <span class="name">All drives</span>
      <span class="sub">{formatCount(total)}</span>
    </span>
    <span class="bar"><span class="fill all" style:width="100%"></span></span>
  </button>
  {#each drives.catalogued as v (v.id)}
    {@const u = used(v)}
    {@const scanning = v.scanning || !!drives.progress[v.id!]}
    <button
      class="drive"
      class:on={selected === v.id}
      class:offline={!v.online}
      title={v.online ? (v.mount ?? v.label) : `${v.label} is offline. Its files stay browsable.`}
      onclick={() => pick(v.id)}
    >
      <span class="line">
        <span class="dot" class:on={v.online}></span>
        <span class="name">{nameOf(v)}</span>
        <span class="sub">{scanning ? "Scanning…" : v.online ? compact.format(v.fileCount) : "Offline"}</span>
      </span>
      <span class="bar"><span class="fill" class:full={u > NEARLY_FULL} style:width="{u * 100}%"></span></span>
    </button>
  {/each}
  {#if drives.available.length}
    <button class="drive add" onclick={onManage}>
      <span class="line"><span class="name">+ Add a drive</span></span>
      <span class="sub">{drives.available.length} not in the catalog</span>
    </button>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    flex: none;
    gap: 8px;
    padding: 0 24px 12px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .drive {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 9px;
    width: 172px;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: var(--panel);
    text-align: left;
  }
  .drive:hover:not(:disabled) {
    border-color: var(--border);
  }
  .drive.on {
    border-color: var(--accent);
    background: var(--raised);
  }
  .drive.offline {
    opacity: 0.55;
  }
  .add {
    border: 1px dashed var(--border);
    background: none;
    gap: 5px;
  }
  .line {
    display: flex;
    gap: 7px;
    align-items: center;
    min-width: 0;
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.on {
    background: var(--badge-image);
  }
  .name {
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    flex: none;
    margin-left: auto;
    color: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .add .sub {
    margin: 0;
  }
  .bar {
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: var(--border);
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--muted);
  }
  .fill.full {
    background: var(--warning);
  }
  .fill.all {
    background: var(--accent);
  }
</style>
