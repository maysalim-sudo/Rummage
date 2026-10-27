<script lang="ts">
  import { api, originalUrl, thumbUrl, type FileDetails } from "./api";
  import { formatBytes, formatDate, formatDuration, formatTaken } from "./format";

  let {
    fileId,
    refreshKey,
    labels,
    onError,
  }: {
    fileId: number;
    refreshKey: number;
    labels: Map<number, string>;
    onError: (message: string) => void;
  } = $props();

  let details = $state<FileDetails | null>(null);
  let originalFailed = $state(false);

  // Reload when the selection changes, and while processing fills in details.
  $effect(() => {
    const id = fileId;
    refreshKey;
    api.fileDetails(id).then(
      (d) => {
        if (id === fileId) details = d;
      },
      (e) => onError(String(e)),
    );
  });
  $effect(() => {
    fileId;
    originalFailed = false;
  });

  const d = $derived(details?.id === fileId ? details : null);
  const preview = $derived(d ? (!originalFailed && originalUrl(d)) || thumbUrl(d) : null);
  const megapixels = $derived(d?.width && d?.height ? ((d.width * d.height) / 1e6).toFixed(1) : null);
  const exposure = $derived(
    d
      ? [
          d.fNumber && `f/${+d.fNumber.toFixed(1)}`,
          d.exposure,
          d.iso && `ISO ${d.iso}`,
          d.focalMm && `${Math.round(d.focalMm)} mm`,
        ]
          .filter(Boolean)
          .join(" · ")
      : "",
  );

  async function act(action: (id: number) => Promise<void>) {
    try {
      await action(fileId);
    } catch (e) {
      onError(String(e));
    }
  }
</script>

<aside class="inspector">
  {#if d}
    <div class="preview">
      {#if preview}
        <img src={preview} alt={d.name} onerror={() => (originalFailed = true)} />
      {:else}
        <span class="muted">{d.processed ? "No preview available" : "Preview not made yet"}</span>
      {/if}
    </div>

    <h3 title={d.name}>{d.name}</h3>

    <dl>
      <dt>Drive</dt>
      <dd>{labels.get(d.volumeId) ?? "?"}{d.absPath ? "" : " (offline)"}</dd>
      <dt>Folder</dt>
      <dd class="path">{d.dir || "/"}</dd>
      <dt>Size</dt>
      <dd>{formatBytes(d.size)}</dd>
      {#if d.width && d.height}
        <dt>Dimensions</dt>
        <dd>{d.width} × {d.height}{megapixels && d.kind !== "video" ? ` (${megapixels} MP)` : ""}</dd>
      {/if}
      {#if d.takenAt}
        <dt>Taken</dt>
        <dd>{formatTaken(d.takenAt)}</dd>
      {/if}
      <dt>Modified</dt>
      <dd>{formatDate(d.mtime)}</dd>
      {#if d.camera}
        <dt>Camera</dt>
        <dd>{d.camera}</dd>
      {/if}
      {#if d.lens}
        <dt>Lens</dt>
        <dd>{d.lens}</dd>
      {/if}
      {#if exposure}
        <dt>Exposure</dt>
        <dd>{exposure}</dd>
      {/if}
      {#if d.durationMs}
        <dt>Duration</dt>
        <dd>{formatDuration(d.durationMs)}</dd>
      {/if}
      {#if d.codec}
        <dt>Video</dt>
        <dd>{d.codec.toUpperCase()}{d.fps ? ` · ${+d.fps.toFixed(2)} fps` : ""}</dd>
      {/if}
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

    <div class="actions">
      <button class="primary" disabled={!d.absPath} onclick={() => act(api.openFile)}>Open</button>
      <button disabled={!d.absPath} onclick={() => act(api.revealFile)}>Show in folder</button>
    </div>
    {#if !d.absPath}
      <p class="muted">Plug in “{labels.get(d.volumeId)}” to open this file.</p>
    {/if}
  {/if}
</aside>

<style>
  .inspector {
    height: 100%;
    overflow-y: auto;
    padding: 12px;
    background: var(--panel);
    border-left: 1px solid var(--border);
  }
  .preview {
    display: grid;
    place-items: center;
    aspect-ratio: 4 / 3;
    background: var(--bg);
    border-radius: 6px;
    overflow: hidden;
  }
  .preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  h3 {
    margin: 10px 0 6px;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .path {
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
</style>
