<script lang="ts">
  import type { Group, Layout } from "./layout";

  let {
    layout,
    scrollTop,
    viewHeight,
    onJump,
  }: {
    layout: Layout;
    scrollTop: number;
    viewHeight: number;
    onJump: (top: number) => void;
  } = $props();

  const MIN_LABEL_GAP = 20;
  const MIN_THUMB = 28;

  let height = $state(600);
  let track = $state<HTMLDivElement>();
  let dragging = $state(false);
  let pointerY = $state(0);

  const scale = $derived(layout.height > 0 ? height / layout.height : 0);
  const thumbHeight = $derived(Math.max(MIN_THUMB, viewHeight * scale));
  const thumbTop = $derived(Math.min(height - thumbHeight, scrollTop * scale));

  const shortMonth = new Intl.DateTimeFormat(undefined, { month: "short", timeZone: "UTC" });
  const longMonth = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric", timeZone: "UTC" });
  const when = (g: Group) => Date.UTC(g.year, g.month, 15);

  /** Years always get a label; months get one where there is room between them. */
  const marks = $derived.by(() => {
    const years = new Map<number, Group>();
    for (const g of layout.groups) if (!years.has(g.year)) years.set(g.year, g);
    const taken: number[] = [];
    const list: { y: number; text: string; year: boolean }[] = [];
    const place = (g: Group, text: string, year: boolean) => {
      const y = g.top * scale;
      if (y > height - 14 || taken.some((t) => Math.abs(t - y) < MIN_LABEL_GAP)) return;
      taken.push(y);
      list.push({ y, text, year });
    };
    for (const [year, g] of years) place(g, String(year), true);
    for (const g of layout.groups) place(g, shortMonth.format(when(g)), false);
    return list;
  });

  /** The group under the pointer, for the label shown while dragging. */
  const hovered = $derived.by(() => {
    if (!dragging) return null;
    const y = pointerY / (scale || 1);
    let found = layout.groups[0];
    for (const g of layout.groups) {
      if (g.top > y) break;
      found = g;
    }
    return found;
  });

  function jump(clientY: number) {
    if (!track) return;
    pointerY = Math.max(0, Math.min(height, clientY - track.getBoundingClientRect().top));
    onJump(Math.max(0, pointerY / (scale || 1) - viewHeight / 2));
  }

  function onDown(e: PointerEvent) {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    jump(e.clientY);
  }
</script>

<div
  class="timeline"
  role="slider"
  aria-label="Jump to a date"
  aria-valuemin={0}
  aria-valuemax={Math.round(layout.height)}
  aria-valuenow={Math.round(scrollTop)}
  tabindex="-1"
  bind:this={track}
  bind:clientHeight={height}
  onpointerdown={onDown}
  onpointermove={(e) => dragging && jump(e.clientY)}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
>
  {#each marks as m (m.text + m.y)}
    <span class="mark" class:year={m.year} style:top="{m.y}px">{m.text}</span>
  {/each}
  <span class="rail"></span>
  <span class="thumb" style:top="{thumbTop}px" style:height="{thumbHeight}px"></span>
  {#if hovered}
    <span class="bubble" style:top="{pointerY}px">{longMonth.format(when(hovered))}</span>
  {/if}
</div>

<style>
  .timeline {
    position: absolute;
    top: 8px;
    right: 6px;
    bottom: 8px;
    width: 46px;
    cursor: ns-resize;
    touch-action: none;
    user-select: none;
  }
  .mark {
    position: absolute;
    right: 12px;
    color: var(--muted);
    font-size: 10px;
    line-height: 12px;
  }
  .mark.year {
    color: var(--text);
    font-weight: 600;
  }
  .rail {
    position: absolute;
    top: 0;
    right: 2px;
    bottom: 0;
    width: 2px;
    background: var(--border);
  }
  .thumb {
    position: absolute;
    right: 0;
    width: 6px;
    border-radius: 3px;
    background: var(--accent);
  }
  .bubble {
    position: absolute;
    right: 18px;
    padding: 4px 9px;
    border-radius: 6px;
    background: var(--accent);
    color: #06222f;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    transform: translateY(-50%);
  }
</style>
