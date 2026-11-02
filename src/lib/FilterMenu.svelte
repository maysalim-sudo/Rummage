<script lang="ts">
  import MultiSelect, { type Option } from "./MultiSelect.svelte";
  import { durationBuckets, kindOptions, orientationOptions, sideBuckets } from "./filters";

  let {
    kinds = $bindable(),
    sides = $bindable(),
    orientations = $bindable(),
    chosenCameras = $bindable(),
    durations = $bindable(),
    dateFrom = $bindable(),
    dateTo = $bindable(),
    includeMissing = $bindable(),
    cameraOptions,
    active,
  }: {
    kinds: string[];
    sides: string[];
    orientations: string[];
    chosenCameras: string[];
    durations: string[];
    dateFrom: string;
    dateTo: string;
    includeMissing: boolean;
    cameraOptions: Option[];
    /** Whether anything is filtered, to offer "Clear all". */
    active: boolean;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();

  function onWindowPointer(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }

  function clearAll() {
    kinds = sides = orientations = chosenCameras = durations = [];
    dateFrom = dateTo = "";
    includeMissing = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointer} />

<div class="menu" bind:this={root}>
  <button class="add" aria-haspopup="dialog" aria-expanded={open} onclick={() => (open = !open)}>Filter</button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="popover" onkeydown={(e) => e.key === "Escape" && (open = false)}>
      <span class="label">Type</span>
      <MultiSelect placeholder="All types" noun="types" options={kindOptions} bind:selected={kinds} />
      <span class="label">Camera</span>
      <MultiSelect placeholder="Any camera" noun="cameras" options={cameraOptions} bind:selected={chosenCameras} searchable />
      <span class="label">Resolution</span>
      <MultiSelect placeholder="Any resolution" noun="sizes" options={sideBuckets} bind:selected={sides} />
      <span class="label">Orientation</span>
      <MultiSelect placeholder="Any orientation" noun="orientations" options={orientationOptions} bind:selected={orientations} />
      <span class="label">Video length</span>
      <MultiSelect placeholder="Any length" noun="lengths" options={durationBuckets} bind:selected={durations} />
      <span class="label">From</span>
      <input type="date" bind:value={dateFrom} aria-label="From date" />
      <span class="label">To</span>
      <input type="date" bind:value={dateTo} aria-label="To date" />
      <span class="label">Deleted</span>
      <label class="check"><input type="checkbox" bind:checked={includeMissing} /> Show deleted files</label>
      {#if active}
        <button class="clear" onclick={clearAll}>Clear all</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
    flex: none;
  }
  .add {
    height: 28px;
    padding: 0 12px;
    border: 1px dashed var(--border);
    border-radius: 14px;
    background: none;
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
  }
  .add:hover,
  .add[aria-expanded="true"] {
    color: var(--text);
  }
  .popover {
    position: absolute;
    top: calc(100% + 10px);
    left: -8px;
    z-index: 30;
    display: grid;
    grid-template-columns: 92px 230px;
    gap: 8px 10px;
    align-items: center;
    padding: 14px;
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 36px rgb(0 0 0 / 0.45);
  }
  .label {
    color: var(--muted);
    font-size: 12px;
  }
  .popover input[type="date"] {
    width: 100%;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .clear {
    grid-column: 2;
    justify-self: end;
    font-size: 12px;
  }
</style>
