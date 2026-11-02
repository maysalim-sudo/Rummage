<script lang="ts">
  import { api, previewUrl, thumbUrl, videoUrl, type FileCopy, type FileDetails } from "./api";
  import { formatBytes, formatDate, formatDuration, formatTaken } from "./format";
  import type { PagedFiles } from "./paged.svelte";

  let {
    files,
    index = $bindable(),
    refreshKey,
    labels,
    onClose,
    onError,
    onSimilar,
  }: {
    files: PagedFiles;
    /** Position of the shown file in the results. */
    index: number;
    refreshKey: number;
    labels: Map<number, string>;
    onClose: () => void;
    onError: (message: string) => void;
    onSimilar: (id: number) => void;
  } = $props();

  const FILMSTRIP_REACH = 7;

  let details = $state<FileDetails | null>(null);
  let copies = $state<FileCopy[]>([]);
  let previewFailed = $state(false);
  let previewReady = $state(false);
  let video = $state<{ id: number; url: string } | null>(null);
  let videoFailed = $state(false);
  let showDetails = $state(true);

  const row = $derived(files.get(index));
  const fileId = $derived(row?.id ?? null);
  const d = $derived(details?.id === fileId ? details : null);
  const isVideo = $derived(d?.kind === "video");
  const thumb = $derived(d ? thumbUrl(d) : null);
  const preview = $derived(d && !previewFailed ? previewUrl(d) : null);
  const strip = $derived.by(() => {
    const list: number[] = [];
    for (let i = Math.max(0, index - FILMSTRIP_REACH); i <= Math.min(files.total - 1, index + FILMSTRIP_REACH); i++) list.push(i);
    return list;
  });

  const exposure = $derived(
    d
      ? ([
          d.fNumber && `f/${+d.fNumber.toFixed(1)}`,
          d.exposure,
          d.iso && `ISO ${d.iso}`,
          d.focalMm && `${Math.round(d.focalMm)} mm`,
        ].filter(Boolean) as string[])
      : [],
  );

  $effect(() => files.ensure(Math.max(0, index - FILMSTRIP_REACH), index + FILMSTRIP_REACH + 1));

  // Reload when the file changes, and while processing fills in details.
  $effect(() => {
    const id = fileId;
    refreshKey;
    if (id === null) return;
    api.fileDetails(id).then(
      (loaded) => {
        if (id === fileId) details = loaded;
      },
      (e) => onError(String(e)),
    );
  });
  $effect(() => {
    const id = fileId;
    copies = [];
    if (id === null) return;
    api.fileCopies(id).then(
      (list) => {
        if (id === fileId) copies = list;
      },
      () => {}, // Only a hint; the viewer works without it.
    );
  });
  $effect(() => {
    fileId;
    previewFailed = false;
    previewReady = false;
    videoFailed = false;
  });

  // Videos on a plugged-in drive play right here.
  $effect(() => {
    const id = fileId;
    if (id === null || !isVideo || !d?.absPath) {
      video = null;
      return;
    }
    videoUrl(id).then((url) => {
      if (id === fileId) video = url ? { id, url } : null;
    });
  });

  function go(delta: number) {
    index = Math.max(0, Math.min(files.total - 1, index + delta));
  }

  function onKey(e: KeyboardEvent) {
    const target = e.target as Element | null;
    if (target?.closest("video, input")) {
      if (e.key === "Escape") onClose();
      return;
    }
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "Home") index = 0;
    else if (e.key === "End") index = files.total - 1;
    else if (e.key === "d" || e.key === "D") showDetails = !showDetails;
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  async function act(action: (id: number) => Promise<void>) {
    if (fileId === null) return;
    try {
      await action(fileId);
    } catch (e) {
      onError(String(e));
    }
  }

  const copyPlace = (c: FileCopy) => `${labels.get(c.volumeId) ?? "?"}${c.dir ? ` / ${c.dir}` : ""}`;
</script>

<svelte:window onkeydown={onKey} />

<div class="viewer" role="dialog" aria-modal="true" aria-label={row?.name ?? "Viewer"}>
  <div class="main">
    <header>
      <button class="back" onclick={onClose}>← Library</button>
      <span class="title" title={row?.name}>{row?.name ?? ""}</span>
      <span class="position muted">{(index + 1).toLocaleString()} of {files.total.toLocaleString()}</span>
    </header>

    <div class="stage">
      {#if video && d && video.id === d.id && !videoFailed}
        {#key video.url}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={video.url} poster={thumb ?? undefined} controls preload="metadata" onerror={() => (videoFailed = true)}></video>
        {/key}
      {:else if d}
        {#if thumb}
          <img class="under" class:hidden={previewReady} src={thumb} alt="" />
        {/if}
        {#if preview}
          {#key preview}
            <img
              class="over"
              class:hidden={!previewReady}
              src={preview}
              alt={d.name}
              onload={() => (previewReady = true)}
              onerror={() => (previewFailed = true)}
            />
          {/key}
        {:else if !thumb}
          <span class="muted">{d.processed ? "No preview available" : "Preview not made yet"}</span>
        {/if}
      {/if}
      {#if isVideo && videoFailed}
        <p class="note">This video's format can't play here. Use “Open in VLC”.</p>
      {/if}
      <button class="nav prev" aria-label="Previous" disabled={index === 0} onclick={() => go(-1)}>‹</button>
      <button class="nav next" aria-label="Next" disabled={index >= files.total - 1} onclick={() => go(1)}>›</button>
    </div>

    <div class="film" role="group" aria-label="Nearby files">
      {#each strip as i (i)}
        {@const r = files.get(i)}
        {@const url = r ? thumbUrl(r) : null}
        <button class:current={i === index} aria-label={r?.name ?? "Loading"} onclick={() => (index = i)}>
          {#if url}<img src={url} alt="" decoding="async" draggable="false" />{/if}
        </button>
      {/each}
    </div>
  </div>

  {#if showDetails}
    <aside aria-label="Details">
      <div class="head">
        <h2>Details</h2>
        <button class="close" aria-label="Close viewer" title="Close (Esc)" onclick={onClose}>✕</button>
      </div>
      {#if d}
        {#if d.camera}
          <div class="camera">{d.camera}</div>
        {/if}
        {#if d.lens}
          <div class="lens muted">{d.lens}</div>
        {/if}
        {#if exposure.length}
          <div class="exposure">
            {#each exposure as e (e)}<span>{e}</span>{/each}
          </div>
        {/if}
        <dl>
          {#if d.takenAt}
            <dt>Taken</dt>
            <dd>{formatTaken(d.takenAt)}</dd>
          {/if}
          <dt>Size</dt>
          <dd>
            {[
              d.width && d.height ? `${d.width} × ${d.height}` : "",
              d.width && d.height && !isVideo ? `${((d.width * d.height) / 1e6).toFixed(1)} MP` : "",
              formatBytes(d.size),
            ]
              .filter(Boolean)
              .join(" · ")}
          </dd>
          {#if d.durationMs}
            <dt>Duration</dt>
            <dd>{formatDuration(d.durationMs)}{d.codec ? ` · ${d.codec.toUpperCase()}` : ""}{d.fps ? ` · ${+d.fps.toFixed(2)} fps` : ""}</dd>
          {/if}
          <dt>Where</dt>
          <dd class="path">{labels.get(d.volumeId) ?? "?"}{d.dir ? ` / ${d.dir}` : ""}{d.absPath ? "" : " (offline)"}</dd>
          <dt>Modified</dt>
          <dd>{formatDate(d.mtime)}</dd>
          {#if d.gpsLat !== null && d.gpsLon !== null}
            <dt>Location</dt>
            <dd>{d.gpsLat.toFixed(5)}, {d.gpsLon.toFixed(5)}</dd>
          {/if}
          {#if !d.processed}
            <dt>Status</dt>
            <dd class="muted">Waiting for details and thumbnail…</dd>
          {:else if d.processError}
            <dt>Note</dt>
            <dd class="muted">{d.processError}</dd>
          {/if}
        </dl>
        {#if copies.length}
          <div class="copies">
            <strong>{copies.length + 1} copies of this file</strong>
            {#each copies.slice(0, 3) as c (`${c.volumeId}/${c.dir}/${c.name}`)}
              <span title={copyPlace(c)}>{copyPlace(c)}</span>
            {/each}
          </div>
        {/if}
        <button class="primary open" disabled={!d.absPath} onclick={() => act(api.openFile)}>Open in default app</button>
        <div class="row">
          <button disabled={!d.absPath} onclick={() => act(api.revealFile)}>Show in folder</button>
          {#if isVideo}
            <button disabled={!d.absPath} onclick={() => act(api.openInVlc)}>Open in VLC</button>
          {:else}
            <button onclick={() => fileId !== null && onSimilar(fileId)}>Find similar</button>
          {/if}
        </div>
        {#if !d.absPath}
          <p class="muted">Plug in “{labels.get(d.volumeId)}” to open this file.</p>
        {/if}
        <p class="keys muted">Esc closes · ← → next file · D hides details</p>
      {/if}
    </aside>
  {/if}
</div>

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    background: #08090a;
    color: #f2f3f4;
  }
  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  header {
    display: grid;
    flex: none;
    grid-template-columns: 1fr auto 1fr;
    gap: 16px;
    align-items: center;
    height: 48px;
    padding: 0 16px;
  }
  .back {
    justify-self: start;
    border: none;
    background: none;
    color: inherit;
    font-weight: 500;
  }
  .title {
    max-width: 44vw;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .position {
    font-variant-numeric: tabular-nums;
  }
  .stage {
    position: relative;
    display: grid;
    flex: 1;
    min-height: 0;
    place-items: center;
    padding: 0 72px;
  }
  .stage img,
  .stage video {
    grid-area: 1 / 1;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    min-height: 0;
  }
  .stage .under {
    width: 100%;
    height: 100%;
    filter: blur(2px);
  }
  .hidden {
    display: none;
  }
  .note {
    grid-area: 1 / 1;
    align-self: end;
    padding: 6px 12px;
    border-radius: 6px;
    background: rgb(0 0 0 / 0.7);
  }
  .nav {
    position: absolute;
    top: 50%;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0 0 3px;
    border: none;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.08);
    color: inherit;
    font-size: 24px;
    transform: translateY(-50%);
  }
  .nav:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.18);
  }
  .prev {
    left: 16px;
  }
  .next {
    right: 16px;
  }
  .film {
    display: flex;
    flex: none;
    gap: 8px;
    justify-content: center;
    height: 104px;
    padding: 12px;
    overflow: hidden;
    border-top: 1px solid #1d2125;
    background: #0e1012;
  }
  .film button {
    flex: none;
    width: 80px;
    height: 80px;
    padding: 0;
    overflow: hidden;
    border: none;
    border-radius: 6px;
    background: #171a1d;
    opacity: 0.6;
  }
  .film button:hover {
    opacity: 1;
  }
  .film button.current {
    outline: 3px solid var(--accent);
    outline-offset: -3px;
    opacity: 1;
  }
  .film img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  aside {
    flex: none;
    width: 316px;
    padding: 14px 20px 20px;
    overflow-y: auto;
    border-left: 1px solid #2b3036;
    background: #171a1d;
  }
  aside button {
    border-color: #2b3036;
    background: #20242a;
    color: inherit;
  }
  aside button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #06222f;
    font-weight: 600;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .close {
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    border-radius: 50%;
  }
  .camera {
    font-size: 19px;
    font-weight: 600;
  }
  .lens {
    margin-top: 2px;
  }
  .exposure {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }
  .exposure span {
    padding: 8px 11px;
    border-radius: 8px;
    background: #20242a;
    font-weight: 600;
  }
  dl {
    margin: 16px 0 0;
  }
  dt {
    color: #8d96a0;
    font-size: 11px;
  }
  dd {
    margin: 2px 0 11px;
    overflow-wrap: anywhere;
  }
  .path {
    font-size: 12px;
  }
  .copies {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 4px;
    padding: 11px 13px;
    border-radius: 8px;
    background: #2c2413;
    color: #c9b083;
    font-size: 12px;
  }
  .copies strong {
    color: #f1c36b;
  }
  .copies span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .open {
    width: 100%;
    height: 40px;
    margin-top: 14px;
    border-radius: 8px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-top: 8px;
  }
  .row button {
    height: 36px;
    border-radius: 8px;
    font-size: 12px;
  }
  .keys {
    margin: 14px 0 0;
    font-size: 11px;
  }
  aside p.muted,
  .muted {
    color: #8d96a0;
  }
</style>
