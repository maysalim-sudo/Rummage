// The current file query's results, loaded in pages on demand. Shared by the list
// and grid views so switching views keeps what is already loaded.
import { api, type FileQuery, type FileRow } from "./api";

export const PAGE_SIZE = 200;

export class PagedFiles {
  total = $state(0);
  /** True once the first page of the current query has arrived. */
  loaded = $state(false);
  /** Bumped on every new query; views scroll back to the top when it changes. */
  queryVersion = $state(0);
  pages = $state<Record<number, FileRow[]>>({});

  #query: FileQuery = {};
  #generation = 0;
  #loading = new Set<number>();
  /** Pages loaded for the current generation (others may be shown but are stale). */
  #fresh = new Set<number>();
  #range: [number, number] = [0, 0];
  #onError: (message: string) => void;

  constructor(onError: (message: string) => void) {
    this.#onError = onError;
  }

  setQuery(query: FileQuery) {
    this.#query = query;
    this.#restart();
    this.pages = {};
    this.loaded = false;
    this.queryVersion++;
    this.ensure(0, PAGE_SIZE);
  }

  /** Reloads the visible pages, keeping the stale rows on screen until fresh ones arrive. */
  refresh() {
    this.#restart();
    this.ensure(...this.#range);
  }

  get(index: number): FileRow | undefined {
    return this.pages[Math.floor(index / PAGE_SIZE)]?.[index % PAGE_SIZE];
  }

  /** Makes sure rows first..last (exclusive) are loaded. */
  ensure(first: number, last: number) {
    this.#range = [first, last];
    const firstPage = Math.floor(first / PAGE_SIZE);
    const lastPage = Math.floor(Math.max(first, last - 1) / PAGE_SIZE);
    for (let p = firstPage; p <= lastPage; p++) {
      if (p > 0 && p * PAGE_SIZE >= this.total) break;
      if (!this.#fresh.has(p)) this.#load(p, this.#generation);
    }
  }

  #restart() {
    this.#generation++;
    this.#loading.clear();
    this.#fresh.clear();
  }

  async #load(page: number, generation: number) {
    if (this.#loading.has(page)) return;
    this.#loading.add(page);
    try {
      const result = await api.queryFiles(this.#query, page * PAGE_SIZE, PAGE_SIZE);
      if (generation !== this.#generation) return; // replaced by a newer query or refresh
      this.#fresh.add(page);
      this.total = result.total;
      this.pages[page] = result.rows;
      this.loaded = true;
    } catch (e) {
      if (generation === this.#generation) this.#onError(String(e));
    } finally {
      if (generation === this.#generation) this.#loading.delete(page);
    }
  }
}
