// Drives, scans and background work: shared by the drive strip, the Drives screen and the
// status bar.
import { api, onJobsProgress, onScanFinished, onScanProgress, type JobsStatus, type ScanProgress, type Volume } from "./api";

/** How far back the processing speed is measured. */
const RATE_WINDOW_MS = 120_000;
const MIN_RATE_SAMPLE_MS = 20_000;

export class Drives {
  volumes = $state<Volume[]>([]);
  progress = $state<Record<number, ScanProgress>>({});
  jobs = $state<JobsStatus>({ paused: false, volumes: [] });
  /** Bumped when folder counts change (a scan finished), so open trees reload. */
  treeKey = $state(0);
  /** Estimated milliseconds of processing left, once there is enough to go on. */
  remainingMs = $state<number | null>(null);

  catalogued = $derived(this.volumes.filter((v) => v.id !== null));
  available = $derived(this.volumes.filter((v) => v.id === null));
  jobsDone = $derived(this.jobs.volumes.reduce((n, p) => n + p.done, 0));
  jobsTotal = $derived(this.jobs.volumes.reduce((n, p) => n + p.total, 0));

  #samples: { at: number; done: number; total: number }[] = [];
  #lastChangeNotice = 0;
  #onCatalogChanged: () => void;
  #onError: (message: string) => void;

  constructor(onCatalogChanged: () => void, onError: (message: string) => void) {
    this.#onCatalogChanged = onCatalogChanged;
    this.#onError = onError;
  }

  /** Starts watching; returns the function that stops it. */
  watch(): () => void {
    this.refresh();
    api.jobsStatus().then((s) => (this.jobs = s));
    // Picks up drives being plugged in or removed.
    const timer = setInterval(() => this.refresh(), 5000);
    const unlisten = [
      onJobsProgress((s) => {
        this.jobs = s;
        this.#measure();
      }),
      onScanProgress((p) => {
        this.progress[p.volumeId] = p;
        this.notifyChanged();
      }),
      onScanFinished((f) => {
        delete this.progress[f.volumeId];
        if (f.error) this.#onError(`Scan failed: ${f.error}`);
        this.treeKey++;
        this.refresh();
        this.notifyChanged(true);
      }),
    ];
    return () => {
      clearInterval(timer);
      unlisten.forEach((p) => p.then((off) => off()));
    };
  }

  async refresh() {
    try {
      this.volumes = await api.listVolumes();
    } catch (e) {
      this.#onError(String(e));
    }
  }

  // Throttled so the files fill in live during a scan without reloading constantly.
  notifyChanged(force = false) {
    const now = Date.now();
    if (force || now - this.#lastChangeNotice > 2000) {
      this.#lastChangeNotice = now;
      this.#onCatalogChanged();
    }
  }

  async run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      this.#onError(String(e));
    }
    await this.refresh();
  }

  #measure() {
    const now = Date.now();
    const done = this.jobsDone;
    const total = this.jobsTotal;
    const last = this.#samples.at(-1);
    // Drives join and leave the list, which moves the sums: start measuring again.
    if (this.jobs.paused || (last && last.total !== total)) this.#samples = [];
    this.#samples.push({ at: now, done, total });
    while (this.#samples.length > 1 && now - this.#samples[0].at > RATE_WINDOW_MS) this.#samples.shift();

    const first = this.#samples[0];
    const elapsed = now - first.at;
    const processed = done - first.done;
    this.remainingMs =
      elapsed >= MIN_RATE_SAMPLE_MS && processed > 0 ? ((total - done) / processed) * elapsed : null;
  }
}
