// The library's filter choices.
import type { Range } from "./api";
import type { Option } from "./MultiSelect.svelte";

export const kindOptions: Option[] = [
  { value: "image", label: "Images" },
  { value: "video", label: "Videos" },
  { value: "raw", label: "RAW photos" },
  { value: "design", label: "Design files (AI, EPS, PDF, Figma)" },
];

/** Longer-side buckets; selecting several matches any of them. */
export const sideBuckets: (Option & Range)[] = [
  { value: "small", label: "Small (under 1280 px)", min: null, max: 1280 },
  { value: "hd", label: "HD (1280–1919)", min: 1280, max: 1920 },
  { value: "fhd", label: "Full HD (1920–3839)", min: 1920, max: 3840 },
  { value: "4k", label: "4K (3840–5999)", min: 3840, max: 6000 },
  { value: "huge", label: "Very large (6000+)", min: 6000, max: null },
];

export const orientationOptions: Option[] = [
  { value: "landscape", label: "Landscape" },
  { value: "portrait", label: "Portrait" },
  { value: "square", label: "Square" },
];

export const durationBuckets: (Option & Range)[] = [
  { value: "short", label: "Under 30 s", min: null, max: 30_000 },
  { value: "medium", label: "30 s – 5 min", min: 30_000, max: 300_000 },
  { value: "long", label: "5 – 30 min", min: 300_000, max: 1_800_000 },
  { value: "verylong", label: "Over 30 min", min: 1_800_000, max: null },
];

export function ranges(buckets: (Option & Range)[], chosen: string[]): Range[] {
  return buckets.filter((b) => chosen.includes(b.value)).map(({ min, max }) => ({ min, max }));
}
