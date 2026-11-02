<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import type { SvelteSet } from "svelte/reactivity";
  import { api, dupesApi, type FileDetails } from "./api";
  import { formatCount } from "./format";

  let {
    selection,
    onCompare,
    onSimilar,
    onTrashed,
    onError,
  }: {
    selection: SvelteSet<number>;
    onCompare: (files: FileDetails[]) => void;
    onSimilar: (id: number) => void;
    onTrashed: (message: string) => void;
    onError: (message: string) => void;
  } = $props();

  const MAX_COMPARE = 4;

  const ids = $derived([...selection]);
  const one = $derived(ids.length === 1 ? ids[0] : null);

  async function act(action: () => Promise<void>) {
    try {
      await action();
    } catch (e) {
      onError(String(e));
    }
  }

  async function trash() {
    const ok = await ask(
      `Move the selected files that have another copy to the Recycle Bin? A file that is the last copy stays where it is.`,
      { title: "Trash copies", kind: "warning" },
    );
    if (!ok) return;
    const report = await dupesApi.trash(ids);
    const kept = report.skipped.length;
    const first = report.skipped[0]?.[1];
    onTrashed(
      `Moved ${formatCount(report.trashed.length)} to the Recycle Bin.` +
        (kept ? ` Kept ${formatCount(kept)}: ${first}${kept > 1 ? ", or a similar reason" : ""}.` : ""),
    );
  }
</script>

<div class="bar" role="toolbar" aria-label="Selection">
  <span class="count">{formatCount(ids.length)} selected</span>
  <button
    disabled={ids.length < 2 || ids.length > MAX_COMPARE}
    title="Pick 2 to {MAX_COMPARE} files to compare"
    onclick={() => act(async () => onCompare(await Promise.all(ids.map(api.fileDetails))))}>Compare</button
  >
  <button disabled={one === null} title="Pick one picture" onclick={() => one !== null && onSimilar(one)}>Find similar</button>
  <button disabled={one === null} title="Pick one file" onclick={() => one !== null && act(() => api.revealFile(one))}>Show in folder</button>
  <button class="trash" onclick={() => act(trash)}>Trash copies</button>
  <button class="close" aria-label="Clear selection" onclick={() => selection.clear()}>✕</button>
</div>

<style>
  .bar {
    position: absolute;
    bottom: 18px;
    left: 50%;
    z-index: 10;
    display: flex;
    gap: 2px;
    align-items: center;
    height: 46px;
    padding: 0 8px 0 20px;
    border: 1px solid var(--border);
    border-radius: 23px;
    background: var(--raised);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.5);
    transform: translateX(-50%);
    white-space: nowrap;
  }
  .count {
    margin-right: 12px;
    font-weight: 600;
  }
  button {
    padding: 6px 11px;
    border: none;
    border-radius: 15px;
    background: none;
    font-size: 12px;
    font-weight: 500;
  }
  button:hover:not(:disabled) {
    background: var(--border);
  }
  .trash {
    color: #ff8a95;
  }
  .close {
    color: var(--muted);
  }
</style>
