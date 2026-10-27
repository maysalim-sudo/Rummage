<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    onJobsProgress,
    onScanFinished,
    onScanProgress,
    type JobsStatus,
    type ScanProgress,
    type Volume,
  } from "./api";
  import { formatBytes, formatCount, formatDate } from "./format";
  import FolderTree from "./FolderTree.svelte";

  let {
    volumes = $bindable([]),
    selected = $bindable(null),
    folder = $bindable(null),
    includeMissing,
    onCatalogChanged,
    onError,
  }: {
    volumes?: Volume[];
    selected?: number | null;
    /** Selected folder on the selected drive ('' = root), or null for the whole drive. */
    folder?: string | null;
    includeMissing: boolean;
    onCatalogChanged: () => void;
    onError: (message: string) => void;
  } = $props();

  let expandedDrives = $state<Record<number, boolean>>({});
  /** Bumped when folder counts change (a scan finished), so open trees reload. */
  let treeKey = $state(0);

  function selectDrive(id: number | null) {
    selected = id;
    folder = null;
  }

  function selectFolder(volumeId: number, path: string) {
    selected = volumeId;
    folder = path;
  }

  let progress = $state<Record<number, ScanProgress>>({});
  let jobs = $state<JobsStatus>({ paused: false, volumes: [] });
  let editing = $state<number | null>(null);
  let editLabel = $state("");
  let lastChangeNotice = 0;

  const catalogued = $derived(volumes.filter((v) => v.id !== null));
  const available = $derived(volumes.filter((v) => v.id === null));
  const jobsDone = $derived(jobs.volumes.reduce((n, p) => n + p.done, 0));
  const jobsTotal = $derived(jobs.volumes.reduce((n, p) => n + p.total, 0));
  const labelOf = (id: number) => volumes.find((v) => v.id === id)?.label ?? "Drive";
  const percent = (p: { done: number; total: number }) =>
    `${p.total ? Math.floor((p.done / p.total) * 100) : 100}%`;

  function startRename(v: Volume) {
    editing = v.id;
    editLabel = v.label;
  }

  async function saveRename() {
    if (editing === null) return;
    const id = editing;
    editing = null;
    await run(() => api.renameVolume(id, editLabel));
    notifyChanged(true);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

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
    api.jobsStatus().then((s) => (jobs = s));
    // Picks up drives being plugged in or removed.
    const timer = setInterval(refresh, 5000);
    const unlisten = [
      onJobsProgress((s) => (jobs = s)),
      onScanProgress((p) => {
        progress[p.volumeId] = p;
        notifyChanged();
      }),
      onScanFinished((f) => {
        delete progress[f.volumeId];
        if (f.error) onError(`Scan failed: ${f.error}`);
        treeKey++;
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
    if (selected === volume.id) selectDrive(null);
    await run(() => api.removeVolume(volume.id!));
    notifyChanged(true);
  }

  function usage(v: Volume) {
    return v.totalBytes ? `${formatBytes(v.totalBytes - v.freeBytes)} of ${formatBytes(v.totalBytes)}` : "";
  }
</script>

<nav>
  <h2>Drives</h2>
  <button class="item" class:active={selected === null} onclick={() => selectDrive(null)}>
    <span class="name">All drives</span>
  </button>

  {#each catalogued as v (v.id)}
    {@const p = progress[v.id!]}
    <div class="drive" class:offline={!v.online}>
      {#if editing === v.id}
        <form class="item static" onsubmit={(e) => (e.preventDefault(), saveRename())}>
          <input
            class="rename"
            bind:value={editLabel}
            use:focus
            onblur={saveRename}
            onkeydown={(e) => e.key === "Escape" && (editing = null)}
            aria-label="Drive name"
          />
        </form>
      {:else}
        <div class="drive-row">
          <button
            class="chevron"
            aria-label={expandedDrives[v.id!] ? `Hide folders of ${v.label}` : `Show folders of ${v.label}`}
            aria-expanded={!!expandedDrives[v.id!]}
            onclick={() => (expandedDrives[v.id!] = !expandedDrives[v.id!])}
            >{expandedDrives[v.id!] ? "▾" : "▸"}</button
          >
          <button
            class="item"
            class:active={selected === v.id && folder === null}
            onclick={() => selectDrive(v.id)}
            ondblclick={() => startRename(v)}
            title={`${v.mount ?? ""} — double-click to rename`}
          >
            <span class="dot" class:on={v.online} title={v.online ? "Plugged in" : "Offline"}></span>
            <span class="name">{v.label}</span>
            <span class="count">{formatCount(v.fileCount)}</span>
          </button>
        </div>
      {/if}
      {#if expandedDrives[v.id!]}
        <div class="tree">
          <FolderTree
            volumeId={v.id!}
            parent=""
            depth={0}
            refreshKey={treeKey}
            {includeMissing}
            selectedPath={selected === v.id ? folder : null}
            onSelect={selectFolder}
          />
        </div>
      {/if}
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
          <button onclick={() => startRename(v)}>Rename</button>
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

  {#if jobs.volumes.length || jobs.paused}
    <section class="jobs">
      <h2>Thumbnails &amp; details</h2>
      {#if jobs.volumes.length}
        <div class="details">
          {formatCount(jobsDone)} of {formatCount(jobsTotal)} files done{jobs.paused ? " · paused" : ""}
        </div>
        {#each jobs.volumes as p (p.volumeId)}
          <div class="job" class:queued={p.state === "queued"}>
            <div class="job-line">
              <span class="job-name">{labelOf(p.volumeId)}</span>
              <span class="muted">
                {p.state === "queued" ? "queued · " : ""}{percent(p)}
              </span>
            </div>
            <progress max={p.total} value={p.done}></progress>
            <div class="muted job-count">{formatCount(p.done)} of {formatCount(p.total)}</div>
          </div>
        {/each}
      {:else}
        <div class="details muted">Paused</div>
      {/if}
      <div class="actions">
        <button onclick={() => run(() => api.setJobsPaused(!jobs.paused))}>{jobs.paused ? "Resume" : "Pause"}</button>
      </div>
    </section>
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
  .rename {
    width: 100%;
  }
  .drive-row {
    display: flex;
    align-items: center;
  }
  .drive-row .item {
    padding-left: 2px;
  }
  .chevron {
    flex: none;
    width: 18px;
    padding: 0;
    border: none;
    background: none;
    color: var(--muted);
  }
  .tree {
    margin: 2px 0 6px 18px;
  }
  .jobs {
    margin-top: 16px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }
  .jobs .details {
    padding-left: 8px;
  }
  .jobs .actions {
    padding-left: 8px;
  }
  .job {
    padding: 6px 8px 0;
  }
  .job-line {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }
  .job-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .job-count {
    font-size: 11px;
  }
  .job.queued progress {
    opacity: 0.5;
  }
  progress {
    width: 100%;
    height: 6px;
    margin: 2px 0;
    accent-color: var(--accent);
  }
</style>
