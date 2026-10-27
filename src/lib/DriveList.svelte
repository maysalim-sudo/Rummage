<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, onScanFinished, onScanProgress, type ScanProgress, type Volume } from "./api";
  import { formatBytes, formatCount, formatDate } from "./format";

  let {
    volumes = $bindable([]),
    selected = $bindable(null),
    onCatalogChanged,
    onError,
  }: {
    volumes?: Volume[];
    selected?: number | null;
    onCatalogChanged: () => void;
    onError: (message: string) => void;
  } = $props();

  let progress = $state<Record<number, ScanProgress>>({});
  let lastChangeNotice = 0;

  const catalogued = $derived(volumes.filter((v) => v.id !== null));
  const available = $derived(volumes.filter((v) => v.id === null));

  async function refresh() {
    try {
      volumes = await api.listVolumes();
    } catch (e) {
      onError(String(e));
    }
  }

  // Throttled so the file list fills in live during a scan without reloading constantly.
  function notifyChanged(force = false) {
    const now = Date.now();
    if (force || now - lastChangeNotice > 2000) {
      lastChangeNotice = now;
      onCatalogChanged();
    }
  }

  onMount(() => {
    refresh();
    // Picks up drives being plugged in or removed.
    const timer = setInterval(refresh, 5000);
    const unlisten = [
      onScanProgress((p) => {
        progress[p.volumeId] = p;
        notifyChanged();
      }),
      onScanFinished((f) => {
        delete progress[f.volumeId];
        if (f.error) onError(`Scan failed: ${f.error}`);
        refresh();
        notifyChanged(true);
      }),
    ];
    return () => {
      clearInterval(timer);
      unlisten.forEach((p) => p.then((off) => off()));
    };
  });

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      onError(String(e));
    }
    await refresh();
  }

  async function add(volume: Volume) {
    await run(async () => {
      const id = await api.addVolume(volume.mount!);
      await api.startScan(id);
    });
  }

  async function remove(volume: Volume) {
    const ok = await ask(
      `Remove "${volume.label}" from the catalog? Files on the drive are not touched; you can add it again later.`,
      { title: "Remove drive", kind: "warning" },
    );
    if (!ok) return;
    if (selected === volume.id) selected = null;
    await run(() => api.removeVolume(volume.id!));
    notifyChanged(true);
  }

  function usage(v: Volume) {
    return v.totalBytes ? `${formatBytes(v.totalBytes - v.freeBytes)} of ${formatBytes(v.totalBytes)}` : "";
  }
</script>

<nav>
  <h2>Drives</h2>
  <button class="item" class:active={selected === null} onclick={() => (selected = null)}>
    <span class="name">All drives</span>
  </button>

  {#each catalogued as v (v.id)}
    {@const p = progress[v.id!]}
    <div class="drive" class:offline={!v.online}>
      <button class="item" class:active={selected === v.id} onclick={() => (selected = v.id)}>
        <span class="dot" class:on={v.online} title={v.online ? "Plugged in" : "Offline"}></span>
        <span class="name">{v.label}</span>
        <span class="count">{formatCount(v.fileCount)}</span>
      </button>
      <div class="details muted">
        {#if v.online}{usage(v)} · {v.fsType}{v.rotational ? " · HDD" : ""}{:else}Offline — plug in to scan or open files{/if}
      </div>
      {#if v.roots.some((r) => r !== "")}
        <div class="details muted">Scans: {v.roots.map((r) => r || "whole drive").join(", ")}</div>
      {/if}
      {#if p || v.scanning}
        <div class="details">
          Scanning… {formatCount(p?.stats.files_seen ?? 0)} files
          {#if p?.currentDir}<div class="current muted" title={p.currentDir}>{p.currentDir}</div>{/if}
        </div>
        <div class="actions"><button onclick={() => run(() => api.cancelScan(v.id!))}>Cancel</button></div>
      {:else}
        <div class="details muted">
          {v.lastScanAt ? `Scanned ${formatDate(v.lastScanAt)}` : "Not scanned yet"}
        </div>
        <div class="actions">
          <button disabled={!v.online} onclick={() => run(() => api.startScan(v.id!))}>Rescan</button>
          <button class="danger" onclick={() => remove(v)}>Remove</button>
        </div>
      {/if}
    </div>
  {/each}

  {#if available.length}
    <h2>Not in catalog</h2>
    {#each available as v (v.mount)}
      <div class="drive">
        <div class="item static">
          <span class="dot on"></span>
          <span class="name" title={v.mount}>{v.label}</span>
        </div>
        <div class="details muted">{usage(v)} · {v.fsType}</div>
        <div class="actions"><button onclick={() => add(v)}>Add &amp; scan</button></div>
      </div>
    {/each}
  {/if}
</nav>

<style>
  nav {
    height: 100%;
    overflow-y: auto;
    padding: 12px 8px;
    background: var(--sidebar);
    border-right: 1px solid var(--border);
  }
  h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin: 12px 8px 6px;
  }
  h2:first-child {
    margin-top: 0;
  }
  .drive {
    margin-bottom: 8px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    border: none;
    background: none;
    padding: 6px 8px;
    border-radius: 4px;
  }
  .item:not(.static):hover {
    background: var(--row-hover);
  }
  .item.active {
    background: var(--row-selected);
  }
  .name {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    flex: none;
  }
  .dot.on {
    background: var(--badge-image);
  }
  .offline .name {
    color: var(--muted);
  }
  .details {
    padding: 0 8px 0 24px;
    font-size: 12px;
  }
  .current {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .actions {
    display: flex;
    gap: 6px;
    padding: 4px 8px 0 24px;
  }
  .actions button {
    font-size: 12px;
    padding: 2px 8px;
  }
</style>
