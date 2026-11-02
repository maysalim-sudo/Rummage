// The justified photo grid's layout: rows of real picture shapes, optionally grouped by
// month. Works from typed arrays so hundreds of thousands of files lay out in milliseconds.

/** Files without a known shape yet get this ratio; the extremes keep rows from being all panorama. */
const DEFAULT_RATIO = 1.5;
const MIN_RATIO = 0.6;
const MAX_RATIO = 2.4;
const MAX_STRETCH = 1.45;
export const GROUP_HEADER = 56;

export interface LayoutData {
  dates: Int32Array;
  ratios: Float32Array;
}

export interface Group {
  /** Index of the group's first file. */
  index: number;
  count: number;
  top: number;
  year: number;
  /** 0–11. */
  month: number;
}

export interface Layout {
  rowCount: number;
  /** First file of each row; has one extra entry, the file count. */
  rowStart: Int32Array;
  rowTop: Float64Array;
  rowHeight: Float32Array;
  groups: Group[];
  height: number;
}

/** Reads the 8-bytes-per-file buffer from `file_layout`: an i32 date and an f32 ratio. */
export function parseLayout(buffer: ArrayBuffer): LayoutData {
  const view = new DataView(buffer);
  const count = Math.floor(buffer.byteLength / 8);
  const dates = new Int32Array(count);
  const ratios = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    dates[i] = view.getInt32(i * 8, true);
    ratios[i] = view.getFloat32(i * 8 + 4, true);
  }
  return { dates, ratios };
}

export function displayRatio(ratio: number): number {
  return ratio > 0 ? Math.min(MAX_RATIO, Math.max(MIN_RATIO, ratio)) : DEFAULT_RATIO;
}

/** Stored dates are the camera's clock, so months are read as UTC. */
function monthKey(date: number): number {
  const d = new Date(date * 1000);
  return d.getUTCFullYear() * 12 + d.getUTCMonth();
}

export function computeLayout({ dates, ratios }: LayoutData, width: number, target: number, gap: number, grouped: boolean): Layout {
  const count = dates.length;
  const rowStart = new Int32Array(count + 1);
  const rowTop = new Float64Array(count);
  const rowHeight = new Float32Array(count);
  const groups: Group[] = [];
  let rows = 0;
  let y = 0;

  let start = 0;
  while (start < count) {
    // One group, or everything when ungrouped.
    let end = count;
    if (grouped) {
      const key = monthKey(dates[start]);
      end = start + 1;
      while (end < count && monthKey(dates[end]) === key) end++;
      groups.push({
        index: start,
        count: end - start,
        top: y,
        year: Math.floor(key / 12),
        month: key % 12,
      });
      y += GROUP_HEADER;
    }

    let first = start;
    let sum = 0;
    for (let i = start; i < end; i++) {
      sum += displayRatio(ratios[i]);
      const n = i - first + 1;
      const full = (width - gap * (n - 1)) / sum <= target;
      if (full || i === end - 1) {
        // A short last row keeps the target height instead of stretching across.
        const height = full ? Math.min((width - gap * (n - 1)) / sum, target * MAX_STRETCH) : target;
        rowStart[rows] = first;
        rowTop[rows] = y;
        rowHeight[rows] = height;
        rows++;
        y += height + gap;
        first = i + 1;
        sum = 0;
      }
    }
    if (grouped) y += 12;
    start = end;
  }
  rowStart[rows] = count;
  return { rowCount: rows, rowStart, rowTop, rowHeight, groups, height: y };
}

/** The last row starting at or above `y`. */
export function rowAt(layout: Layout, y: number): number {
  let low = 0;
  let high = layout.rowCount - 1;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (layout.rowTop[mid] <= y) low = mid;
    else high = mid - 1;
  }
  return low;
}

/** The row holding file `index`. */
export function rowOf(layout: Layout, index: number): number {
  let low = 0;
  let high = layout.rowCount - 1;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (layout.rowStart[mid] <= index) low = mid;
    else high = mid - 1;
  }
  return low;
}
