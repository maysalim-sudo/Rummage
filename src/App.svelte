<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    api,
    onJobsProgress,
    type CameraCount,
    type FileQuery,
    type FileRow,
    type MediaKind,
    type Orientation,
    type Range,
    type SetupInfo,
    type Sort,
    type Volume,
  } from "./lib/api";
  import { formatCount } from "./lib/format";
  import { PagedFiles } from "./lib/paged.svelte";
  import Setup from "./lib/Setup.svelte";
  import DriveList from "./lib/DriveList.svelte";
  import FileList from "./lib/FileList.svelte";
  import FileGrid from "./lib/FileGrid.svelte";
  import Inspector from "./lib/Inspector.svelte";
  import DupesView from "./lib/DupesView.svelte";
  import MultiSelect, { type Option } from "./lib/MultiSelect.svelte";

  type View = "grid" | "list";

  const kindOptions: Option[] = [
    { value: "image", label: "Images" },
    { value: "video", label: "Videos" },
    { value: "raw", label: "RAW photos" },
    { value: "design", label: "Design files (AI, EPS, PDF, Figma)" },
  ];

  /** Longer-side buckets; selecting several matches any of them. */
  const sideBuckets: (Option & Range)[] = [
    { value: "small", label: "Small (under 1280 px)", min: null, max: 1280 },
    { value: "hd", label: "HD (1280–1919)", min: 1280, max: 1920 },
    { value: "fhd", label: "Full HD (1920–3839)", min: 1920, max: 3840 },
    { value: "4k", label: "4K (3840–5999)", min: 3840, max: 6000 },
    { value: "huge", label: "Very large (6000+)", min: 6000, max: null },
  ];

  const orientationOptions: Option[] = [
    { value: "landscape", label: "Landscape" },
    { value: "portrait", label: "Portrait" },
    { value: "square", label: "Square" },
  ];

  const durationBuckets: (Option & Range)[] = [
    { value: "short", label: "Under 30 s", min: null, max: 30_000 },
    { value: "medium", label: "30 s – 5 min", min: 30_000, max: 300_000 },
    { value: "long", label: "5 – 30 min", min: 300_000, max: 1_800_000 },
    { value: "verylong", label: "Over 30 min", min: 1_800_000, max: null },
  ];

  function ranges(buckets: (Option & Range)[], chosen: string[]): Range[] {
    return buckets.filter((b) => chosen.includes(b.value)).map(({ min, max }) => ({ min, max }));
  }

  let setup = $state<SetupInfo | null>(null);
  let volumes = $state<Volume[]>([]);
  let cameras = $state<CameraCount[]>([]);
  let error = $state("");

  // Filters
  let selectedVolume = $state<number | null>(null);
  let selectedFolder = $state<string | null>(null);
  let includeSubfolders = $state(true);
  let text = $state("");
  let searchText = $state("");
  let kinds = $state<string[]>([]);
  let sort = $state<Sort>("newest");
  let includeMissing = $state(false);
  let sides = $state<string[]>([]);
  let orientations = $state<string[]>([]);
  let chosenCameras = $state<string[]>([]);
  let dateFrom = $state("");
  let dateTo = $state("");
  let durations = $state<string[]>([]);

  // View
  let mode = $state<"browse" | "dupes">("browse");
  let view = $state<View>(loadPref("view", "grid") as View);
  let tileSize = $state(Number(loadPref("tileSize", "180")));
  let selected = $state<{ id: number; index: number } | null>(null);
  let refreshKey = $state(0);

  const files = new PagedFiles(showError);

  const labels = $derived(new Map(volumes.filter((v) => v.id !== null).map((v) => [v.id!, v.label])));

  /** "2024-05-01" → unix seconds at the start (or end) of that day, in camera-clock time. */
  function day(value: string, endOfDay = false): number | null {
    if (!value) return null;
    const [y, m, d] = value.split("-").map(Number);
    return Date.UTC(y, m - 1, d) / 1000 + (endOfDay ? 86_399 : 0);
  }

  const query = $derived<FileQuery>({
    text: searchText,
    volumeId: selectedVolume,
    kinds: kinds as MediaKind[],
    includeMissing,
    sort,
    sideRanges: ranges(sideBuckets, sides),
    orientations: orientations as Orientation[],
    cameras: chosenCameras,
    dateFrom: day(dateFrom),
    dateTo: day(dateTo, true),
    durationRanges: ranges(durationBuckets, durations),
    folder: selectedVolume !== null ? selectedFolder : null,
    directOnly: selectedFolder !== null && !includeSubfolders,
  });

  const cameraOptions = $derived<Option[]>(cameras.map((c) => ({ value: c.camera, label: c.camera, count: c.count })));

  /** What is narrowing the results, for the "no files match" message. */
  const activeFilters = $derived.by(() => {
    const list: string[] = [];
    const pick = (opts: Option[], chosen: string[]) => chosen.map((v) => opts.find((o) => o.value === v)?.label ?? v);
    if (selectedVolume !== null) list.push(`drive “${labels.get(selectedVolume) ?? "?"}”`);
    if (selectedFolder) list.push(`folder “${selectedFolder}”${includeSubfolders ? "" : " (no subfolders)"}`);
    if (searchText) list.push(`search “${searchText}”`);
    if (kinds.length) list.push(pick(kindOptions, kinds).join(" or "));
    if (sides.length) list.push(pick(sideBuckets, sides).join(" or "));
    if (orientations.length) list.push(pick(orientationOptions, orientations).join(" or "));
    if (chosenCameras.length) list.push(chosenCameras.join(" or "));
    if (durations.length) list.push(`length ${pick(durationBuckets, durations).join(" or ")}`);
    if (dateFrom) list.push(`from ${dateFrom}`);
    if (dateTo) list.push(`to ${dateTo}`);
    return list;
  });

  /** Breadcrumb segments for the selected folder: [label, path]. */
  const crumbs = $derived(
    selectedFolder
      ? selectedFolder.split("/").map((name, i, parts) => [name, parts.slice(0, i + 1).join("/")] as const)
      : [],
  );

  const emptyMessage = $derived(
    activeFilters.length
      ? `No files match: ${activeFilters.join(" · ")}.`
      : "No files yet. Add a drive on the left to scan it.",
  );

  const filtersActive = $derived(
    !!(
      searchText ||
      kinds.length ||
      sides.length ||
      orientations.length ||
      chosenCameras.length ||
      durations.length ||
      dateFrom ||
      dateTo ||
      includeMissing
    ),
  );

  // Camera counts follow the other filters (debounced: it is a grouped count over the catalog).
  $effect(() => {
    const q = query;
    const timer = setTimeout(() => loadCameras(q), 300);
    return () => clearTimeout(timer);
  });

  // untrack: setQuery reads the result count, which must not re-trigger the query.
  $effect(() => {
    const q = query;
    untrack(() => {
      files.setQuery(q);
      selected = null;
    });
  });

  // Search while typing, but not on every keystroke.
  $effect(() => {
    const value = text;
    const timer = setTimeout(() => (searchText = value.trim()), 250);
    return () => clearTimeout(timer);
  });

  $effect(() => savePref("view", view));
  $effect(() => savePref("tileSize", String(tileSize)));

  function loadPref(key: string, fallback: string): string {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  }

  function savePref(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Preferences are a convenience; ignore storage errors.
    }
  }

  function showError(message: string) {
    error = message;
  }

  function catalogChanged() {
    files.refresh();
    refreshKey++;
  }

  function clearFilters() {
    text = searchText = "";
    kinds = sides = orientations = chosenCameras = durations = [];
    dateFrom = dateTo = "";
    includeMissing = false;
  }

  async function open(row: FileRow) {
    try {
      await api.openFile(row.id);
    } catch (e) {
      showError(String(e));
    }
  }

  async function loadCameras(q: FileQuery) {
    try {
      const list = await api.listCameras(q);
      if (q === query) cameras = list; // ignore answers for filters already changed
    } catch {
      // Not critical; the list fills in on the next try.
    }
  }

  onMount(() => {
    let lastRefresh = 0;
    let lastCameras = 0;
    api.getSetup().then((s) => (setup = s));
    // While thumbnails are being made, refresh the visible files every few seconds so they appear.
    const unlisten = onJobsProgress(() => {
      const now = Date.now();
      if (now - lastRefresh > 3000) {
        lastRefresh = now;
        catalogChanged();
      }
      if (now - lastCameras > 20_000) {
        lastCameras = now;
        loadCameras(untrack(() => query));
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  });
</script>

{#if !setup}
  <div></div>
{:else if !setup.catalogDir}
  <Setup info={setup} onDone={async () => (setup = await api.getSetup())} />
{:else}
  <div class="app" class:with-inspector={selected !== null && mode === "browse"}>
    <aside>
      <DriveList
        bind:volumes
        bind:selected={selectedVolume}
        bind:folder={selectedFolder}
        {includeMissing}
        onCatalogChanged={catalogChanged}
        onError={showError}
      />
    </aside>

    <main>
      <div class="toolbar">
        <div class="segmented" role="tablist" aria-label="Mode">
          <button role="tab" aria-selected={mode === "browse"} class:on={mode === "browse"} onclick={() => (mode = "browse")}>Browse</button>
          <button role="tab" aria-selected={mode === "dupes"} class:on={mode === "dupes"} onclick={() => (mode = "dupes")}>Duplicates</button>
        </div>
        {#if mode === "browse"}
          <input class="search" type="search" placeholder="Search file and folder names…" bind:value={text} />
          <div class="segmented" role="group" aria-label="View">
            <button class:on={view === "grid"} onclick={() => (view = "grid")}>Grid</button>
            <button class:on={view === "list"} onclick={() => (view = "list")}>List</button>
          </div>
          {#if view === "grid"}
            <input type="range" min="110" max="360" step="10" bind:value={tileSize} aria-label="Thumbnail size" />
          {/if}
          <select bind:value={sort} aria-label="Sort">
            <option value="newest">Newest first</option>
            <option value="oldest">Oldest first</option>
            <option value="name">Name</option>
            <option value="largest">Largest first</option>
          </select>
          <span class="count muted">{formatCount(files.total)} files</span>
        {/if}
      </div>

      {#if mode === "browse"}
        <div class="toolbar filters">
          <MultiSelect placeholder="All types" noun="types" options={kindOptions} bind:selected={kinds} />
          <MultiSelect placeholder="Any resolution" noun="sizes" options={sideBuckets} bind:selected={sides} />
          <MultiSelect
            placeholder="Any orientation"
            noun="orientations"
            options={orientationOptions}
            bind:selected={orientations}
          />
          <MultiSelect
            placeholder="Any camera"
            noun="cameras"
            options={cameraOptions}
            bind:selected={chosenCameras}
            searchable
          />
          <MultiSelect placeholder="Any length" noun="lengths" options={durationBuckets} bind:selected={durations} />
          <label>From <input type="date" bind:value={dateFrom} /></label>
          <label>To <input type="date" bind:value={dateTo} /></label>
          <label><input type="checkbox" bind:checked={includeMissing} /> Show deleted</label>
          {#if filtersActive}
            <button onclick={clearFilters}>Clear filters</button>
          {/if}
        </div>

        {#if selectedVolume !== null && selectedFolder !== null}
          <nav class="toolbar crumbs" aria-label="Folder">
            <button class="crumb" onclick={() => (selectedFolder = null)}>{labels.get(selectedVolume) ?? "Drive"}</button>
            {#each crumbs as [name, path], i (path)}
              <span class="muted">/</span>
              {#if i === crumbs.length - 1}
                <span class="crumb current">{name}</span>
              {:else}
                <button class="crumb" onclick={() => (selectedFolder = path)}>{name}</button>
              {/if}
            {/each}
            <label class="subfolders"><input type="checkbox" bind:checked={includeSubfolders} /> Include subfolders</label>
            <button onclick={() => (selectedFolder = null)} aria-label="Show the whole drive">✕</button>
          </nav>
        {/if}
      {/if}

      {#if error}
        <div class="error">
          <span>{error}</span>
          <button onclick={() => (error = "")}>Dismiss</button>
        </div>
      {/if}

      <div class="content">
        {#if mode === "dupes"}
          <DupesView {labels} onError={showError} />
        {:else if view === "grid"}
          <FileGrid {files} bind:selected {tileSize} {labels} {emptyMessage} onOpen={open} />
        {:else}
          <FileList {files} bind:selected {labels} {emptyMessage} onOpen={open} />
        {/if}
      </div>
    </main>

    {#if selected && mode === "browse"}
      <Inspector fileId={selected.id} {refreshKey} {labels} onError={showError} />
    {/if}
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    height: 100%;
  }
  .app.with-inspector {
    grid-template-columns: 280px minmax(0, 1fr) 320px;
  }
  aside {
    min-height: 0;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .filters {
    padding-top: 0;
  }
  .search {
    flex: 1;
    min-width: 160px;
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
  .count {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .crumbs {
    gap: 4px;
    padding-top: 0;
  }
  .crumb {
    border: none;
    background: none;
    padding: 2px 4px;
    color: var(--accent);
  }
  .crumb.current {
    color: var(--text);
    font-weight: 600;
  }
  .subfolders {
    margin-left: auto;
  }
  label {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--danger);
    color: #fff;
  }
  .error button {
    background: none;
    border-color: #fff;
    color: #fff;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
</style>
