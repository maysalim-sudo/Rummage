<script lang="ts">
  import { previewUrl, thumbUrl, type FileDetails } from "./api";
  import { formatBytes, formatDay } from "./format";

  let { files, onClose }: { files: FileDetails[]; onClose: () => void } = $props();

  function fact(f: FileDetails) {
    return [f.width && f.height ? `${f.width} × ${f.height}` : "", formatBytes(f.size), formatDay(f.date), f.camera ?? ""]
      .filter(Boolean)
      .join(" · ");
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="compare" role="dialog" aria-modal="true" aria-label="Compare files">
  <header>
    <h2>Comparing {files.length} files</h2>
    <button class="close" aria-label="Close comparison" title="Close (Esc)" onclick={onClose}>✕</button>
  </header>
  <div class="panes" style:grid-template-columns="repeat({files.length === 4 ? 2 : files.length}, 1fr)">
    {#each files as f (f.id)}
      {@const url = previewUrl(f) ?? thumbUrl(f)}
      <figure>
        <div class="frame">
          {#if url}<img src={url} alt={f.name} />{:else}<span>No preview available</span>{/if}
        </div>
        <figcaption>
          <strong title={f.name}>{f.name}</strong>
          <span>{fact(f)}</span>
        </figcaption>
      </figure>
    {/each}
  </div>
</div>

<style>
  .compare {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    background: #08090a;
    color: #f2f3f4;
  }
  header {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    height: 52px;
    padding: 0 16px 0 20px;
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
    background: #20242a;
    color: inherit;
  }
  .panes {
    display: grid;
    flex: 1;
    gap: 8px;
    min-height: 0;
    grid-auto-rows: minmax(0, 1fr);
    padding: 0 12px 12px;
  }
  figure {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    min-height: 0;
    margin: 0;
  }
  .frame {
    display: grid;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    place-items: center;
    border-radius: 6px;
    background: #111316;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  figcaption {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 1px;
    font-size: 12px;
  }
  figcaption strong,
  figcaption span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  figcaption span {
    color: #8d96a0;
    font-size: 11px;
  }
</style>
