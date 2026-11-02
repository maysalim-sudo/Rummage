<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, type Volume } from "./api";
  import type { Drives } from "./drives.svelte";
  import { formatBytes, formatCount, formatDate } from "./format";

  let {
    drives,
    selected = $bindable(),
    folder = $bindable(),
  }: {
    drives: Drives;
    selected: number | null;
    folder: string | null;
  } = $props();

  let editing = $state<number | null>(null);
  let editLabel = $state("");
  let version = $state("");
  getVersion().then((v) => (version = v));

  function startRename(v: Volume) {
    editing = v.id;
    editLabel = v.label;
  }

  async function saveRename() {
    if (editing === null) return;
    const id = editing;
    editing = null;
    await drives.run(() => api.renameVolume(id, editLabel));
    drives.notifyChanged(true);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function add(volume: Volume) {
    await drives.run(async () => {
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
    if (selected === volume.id) {
      selected = null;
      folder = null;
    }
    await drives.run(() => api.removeVolume(volume.id!));
    drives.notifyChanged(true);
  }

  function usage(v: Volume) {
    return v.totalBytes ? `${formatBytes(v.totalBytes - v.freeBytes)} of ${formatBytes(v.totalBytes)}` : "";
  }
</script>

<div class="screen">
  <h1>Drives</h1>
  <p class="muted">Files stay browsable when a drive is unplugged. Name a drive here to replace its letter everywhere.</p>

  <div class="cards">
    {#each drives.catalogued as v (v.id)}
      {@const p = drives.progress[v.id!]}
      <section class="card" class:offline={!v.online}>
        <div class="head">
          <span class="dot" class:on={v.online} title={v.online ? "Plugged in" : "Offline"}></span>
          {#if editing === v.id}
            <form class="rename" onsubmit={(e) => (e.preventDefault(), saveRename())}>
              <input
                bind:value={editLabel}
                use:focus
                onblur={saveRename}
                onkeydown={(e) => e.key === "Escape" && (editing = null)}
                aria-label="Drive name"
              />
            </form>
          {:else}
            <h2 title={v.mount ?? ""}>{v.label}</h2>
          {/if}
          <span class="count muted">{formatCount(v.fileCount)} files</span>
        </div>
        <div class="muted">
          {#if v.online}{v.mount} · {usage(v)} · {v.fsType}{v.rotational ? " · HDD" : ""}{:else}Offline. Plug it in to scan or open files.{/if}
        </div>
        {#if v.roots.some((r) => r !== "")}
          <div class="muted">Scans: {v.roots.map((r) => r || "whole drive").join(", ")}</div>
        {/if}
        {#if p || v.scanning}
          <div>Scanning… {formatCount(p?.stats.files_seen ?? 0)} files</div>
          {#if p?.currentDir}<div class="current muted" title={p.currentDir}>{p.currentDir}</div>{/if}
          <div class="actions"><button onclick={() => drives.run(() => api.cancelScan(v.id!))}>Cancel</button></div>
        {:else}
          <div class="muted">{v.lastScanAt ? `Scanned ${formatDate(v.lastScanAt)}` : "Not scanned yet"}</div>
          <div class="actions">
            <button disabled={!v.online} onclick={() => drives.run(() => api.startScan(v.id!))}>Rescan</button>
            <button onclick={() => startRename(v)}>Rename</button>
            <button class="danger" onclick={() => remove(v)}>Remove</button>
          </div>
        {/if}
      </section>
    {/each}
  </div>

  {#if drives.available.length}
    <h2 class="group">Not in the catalog</h2>
    <div class="cards">
      {#each drives.available as v (v.mount)}
        <section class="card">
          <div class="head">
            <span class="dot on"></span>
            <h2 title={v.mount}>{v.label}</h2>
          </div>
          <div class="muted">{usage(v)} · {v.fsType}</div>
          <div class="actions"><button class="primary" onclick={() => add(v)}>Add &amp; scan</button></div>
        </section>
      {/each}
    </div>
  {/if}

  {#if version}<p class="version muted">Rummage {version}</p>{/if}
</div>

<style>
  .screen {
    height: 100%;
    padding: 24px;
    overflow-y: auto;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  p {
    margin: 0 0 18px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 12px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 14px 16px;
    border-radius: 12px;
    background: var(--panel);
    font-size: 12px;
  }
  .card.offline {
    opacity: 0.65;
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 2px;
  }
  h2 {
    margin: 0;
    overflow: hidden;
    font-size: 14px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h2.group {
    margin: 24px 0 10px;
  }
  .rename {
    flex: 1;
  }
  .rename input {
    width: 100%;
  }
  .count {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.on {
    background: var(--badge-image);
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
    margin-top: 6px;
  }
  .version {
    margin: 24px 0 0;
    font-size: 11px;
  }
</style>
