<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { ask } from "@tauri-apps/plugin-dialog";
  import {
    api,
    dupesApi,
    onDupesFinished,
    onDupesProgress,
    thumbUrl,
    type DupeGroup,
    type DupeProgress,
    type DupeSummary,
    type FileRow,
  } from "./api";
  import { formatBytes, formatCount, formatDay } from "./format";

  let { labels, onError }: { labels: Map<number, string>; onError: (message: string) => void } = $props();

  type Tab = "exact" | "similar";
  const PAGE = 30;

  let tab = $state<Tab>("exact");
  let summary = $state<DupeSummary | null>(null);
  let groups = $state<DupeGroup[]>([]);
  let totalGroups = $state(0);
  let running = $state(false);
  let progress = $state<DupeProgress | null>(null);
  let notice = $state("");
  let busy = $state(false);
  const chosen = new SvelteSet<number>();

  const stageLabel = {
    quickHash: "Comparing the start and end of same-size files",
    fullHash: "Checking possible copies in full",
    fingerprints: "Fingerprinting thumbnails",
    comparing: "Grouping similar photos",
  } as const;

  const chosenFiles = $derived(groups.flatMap((g) => g.files).filter((f) => chosen.has(f.id)));
  const chosenBytes = $derived(chosenFiles.reduce((n, f) => n + f.size, 0));

  async function load(reset = true) {
    try {
      summary = await dupesApi.summary();
      const page = await dupesApi.groups(tab, reset ? 0 : groups.length, PAGE);
      totalGroups = page.total;
      groups = reset ? page.groups : [...groups, ...page.groups];
      if (reset) chosen.clear();
    } catch (e) {
      onError(String(e));
    }
  }

  async function start() {
    notice = "";
    try {
      await dupesApi.start();
      running = true;
      progress = null;
    } catch (e) {
      onError(String(e));
    }
  }

  function switchTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    load();
  }

  /** Every file but the suggested keeper, in each group shown (exact copies only). */
  function selectExtras(list: DupeGroup[]) {
    for (const g of list) for (const f of g.files.slice(1)) if (f.absPath) chosen.add(f.id);
  }

  function toggle(file: FileRow) {
    if (chosen.has(file.id)) chosen.delete(file.id);
    else chosen.add(file.id);
  }

  async function moveToTrash() {
    const count = chosenFiles.length;
    const ok = await ask(
      `Move ${count} file${count === 1 ? "" : "s"} (${formatBytes(chosenBytes)}) to the trash? You can restore them from the trash.`,
      { title: "Move to trash", kind: "warning" },
    );
    if (!ok) return;
    busy = true;
    try {
      const report = await dupesApi.trash([...chosen]);
      const reasons = [...new Set(report.skipped.map(([, why]) => why))];
      notice =
        `Moved ${formatCount(report.trashed.length)} file${report.trashed.length === 1 ? "" : "s"} to the trash, freeing ${formatBytes(report.freed)}.` +
        (report.skipped.length
          ? ` ${formatCount(report.skipped.length)} left alone: ${reasons.join("; ")}.`
          : "");
      await load();
    } catch (e) {
      onError(String(e));
    } finally {
      busy = false;
    }
  }

  async function reveal(file: FileRow) {
    try {
      await api.revealFile(file.id);
    } catch (e) {
      onError(String(e));
    }
  }

  function describe(f: FileRow) {
    return [f.width && f.height ? `${f.width}×${f.height}` : "", formatBytes(f.size), formatDay(f.date)]
      .filter(Boolean)
      .join(" · ");
  }

  onMount(() => {
    load();
    const unlisten = [
      onDupesProgress((p) => {
        running = true;
        progress = p;
      }),
      onDupesFinished((f) => {
        running = false;
        progress = null;
        if (f.error) onError(`Duplicate search failed: ${f.error}`);
        else if (f.cancelled) notice = "Search stopped. What was checked so far is kept; run it again to continue.";
        load();
      }),
    ];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  });
</script>

<section class="dupes">
  <header>
    <div class="segmented" role="tablist">
      <button role="tab" aria-selected={tab === "exact"} class:on={tab === "exact"} onclick={() => switchTab("exact")}>
        Exact copies{summary ? ` (${formatCount(summary.exactGroups)})` : ""}
      </button>
      <button role="tab" aria-selected={tab === "similar"} class:on={tab === "similar"} onclick={() => switchTab("similar")}>
        Similar photos{summary ? ` (${formatCount(summary.similarGroups)})` : ""}
      </button>
    </div>
    {#if summary && summary.exactWasted > 0}
      <span class="muted">{formatBytes(summary.exactWasted)} could be freed by exact copies</span>
    {/if}
    <span class="spacer"></span>
    {#if running}
      <button onclick={() => dupesApi.cancel()}>Stop</button>
    {:else}
      <button class="primary" onclick={start}>{summary && (summary.exactGroups || summary.similarGroups) ? "Search again" : "Find duplicates"}</button>
    {/if}
  </header>

  {#if running}
    <div class="progress">
      <span>{progress ? stageLabel[progress.stage] : "Starting…"}{progress?.total ? ` · ${formatCount(progress.done)} of ${formatCount(progress.total)}` : ""}</span>
      <progress max={progress?.total || 1} value={progress?.done ?? 0}></progress>
    </div>
  {/if}
  {#if notice}<div class="notice">{notice}</div>{/if}
  {#if summary && summary.uncheckedOffline > 0}
    <div class="notice muted">
      {formatCount(summary.uncheckedOffline)} possible copies are on unplugged drives and haven't been checked. Plug them in and search again.
    </div>
  {/if}

  <div class="groups">
    {#if groups.length === 0}
      <div class="empty muted">
        {#if running}Searching…{:else if summary}No {tab === "exact" ? "exact copies" : "similar photos"} found yet. Click “Find duplicates” to search all plugged-in drives.{/if}
      </div>
    {/if}

    {#if tab === "exact" && groups.length > 0}
      <div class="bulk">
        <button onclick={() => selectExtras(groups)}>Select every copy except the first, in the groups shown</button>
      </div>
    {/if}

    {#each groups as g (g.key)}
      <article class="group">
        <div class="group-head">
          <strong>
            {#if tab === "exact"}
              {g.files.length} copies · {formatBytes(g.files[0].size)} each
            {:else}
              {g.files.length} similar images
            {/if}
          </strong>
          {#if tab === "exact"}<span class="muted">{formatBytes(g.wasted)} extra</span>{/if}
          <button class="link" onclick={() => selectExtras([g])}>Select all but the first</button>
        </div>
        <div class="members">
          {#each g.files as f, i (f.id)}
            {@const url = thumbUrl(f)}
            <div class="member" class:chosen={chosen.has(f.id)} class:offline={!f.absPath}>
              <div class="thumb">
                {#if url}<img src={url} alt="" decoding="async" />{:else}<span class="muted">No preview</span>{/if}
                {#if i === 0}<span class="badge">{tab === "exact" ? "Oldest" : "Largest"}</span>{/if}
              </div>
              <div class="name" title={f.name}>{f.name}</div>
              <div class="where muted" title={`${labels.get(f.volumeId) ?? "?"}: ${f.dir || "/"}`}>
                {labels.get(f.volumeId) ?? "?"} · {f.dir || "/"}
              </div>
              <div class="muted small">{describe(f)}</div>
              <div class="member-actions">
                <label class:disabled={!f.absPath} title={f.absPath ? "" : "Plug the drive in to remove this file"}>
                  <input type="checkbox" checked={chosen.has(f.id)} disabled={!f.absPath} onchange={() => toggle(f)} />
                  Remove
                </label>
                <button class="link" disabled={!f.absPath} onclick={() => reveal(f)}>Show</button>
              </div>
            </div>
          {/each}
        </div>
      </article>
    {/each}

    {#if groups.length < totalGroups}
      <div class="more">
        <button onclick={() => load(false)}>Show more ({formatCount(totalGroups - groups.length)} left)</button>
      </div>
    {/if}
  </div>

  {#if chosen.size > 0}
    <footer>
      <span>{formatCount(chosenFiles.length)} selected · {formatBytes(chosenBytes)}</span>
      <span class="spacer"></span>
      <button onclick={() => chosen.clear()}>Clear</button>
      <button class="danger-fill" disabled={busy} onclick={moveToTrash}>Move to trash</button>
    </footer>
  {/if}
</section>

<style>
  .dupes {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--panel);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .segmented {
    display: flex;
  }
  .segmented button {
    border-radius: 0;
  }
  .segmented button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .segmented button:last-child {
    border-radius: 0 4px 4px 0;
    margin-left: -1px;
  }
  .segmented button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .progress progress {
    width: 100%;
    accent-color: var(--accent);
  }
  .notice {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .groups {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
  }
  .empty {
    padding: 40px 0;
    text-align: center;
  }
  .bulk {
    margin-bottom: 12px;
  }
  .group {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px;
    margin-bottom: 12px;
  }
  .group-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 8px;
  }
  .members {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 4px;
  }
  .member {
    flex: none;
    width: 170px;
    border: 2px solid transparent;
    border-radius: 6px;
    padding: 4px;
  }
  .member.chosen {
    border-color: var(--danger);
  }
  .member.offline .thumb img {
    opacity: 0.55;
  }
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    height: 140px;
    background: var(--bg);
    border-radius: 4px;
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .badge {
    position: absolute;
    left: 6px;
    top: 6px;
    padding: 1px 6px;
    font-size: 11px;
    border-radius: 3px;
    color: #fff;
    background: var(--badge-image);
  }
  .name,
  .where {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    margin-top: 4px;
    font-weight: 600;
  }
  .where,
  .small {
    font-size: 12px;
  }
  .where {
    direction: rtl;
    text-align: left;
  }
  .member-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 4px;
  }
  .member-actions label {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .disabled {
    opacity: 0.5;
  }
  button.link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
  }
  .more {
    text-align: center;
    padding: 8px 0 16px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
  }
  .danger-fill {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
</style>
