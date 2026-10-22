// Where each video was left off. Loaded once, then kept in sync locally: the
// player writes through `saveProgress`, cards read through `useProgress`.
import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";

type Entry = { position: number; duration: number };
type Row = Entry & { id: string };

const MIN_POSITION = 15;
const END_MARGIN = 30;

let map = new Map<string, Entry>();
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

function load() {
  // Never rejects: without saved progress videos just start from the beginning.
  loading ??= invoke<Row[]>("progress_list")
    .then((rows) => {
      map = new Map(rows.map((r) => [r.id, { position: r.position, duration: r.duration }]));
      emit();
    })
    .catch((e) => console.warn("progress:", e));
  return loading;
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  void load();
  return () => listeners.delete(l);
};

/** Same rule as the Rust side: a peek or a finished video leaves nothing. */
const keep = (p: number, d: number) => p >= MIN_POSITION && !(d > 0 && (p > d - END_MARGIN || p > d * 0.95));

/** Watched share 0..1, or null for unstarted and finished videos. */
export function useProgress(id: string): number | null {
  return useSyncExternalStore(subscribe, () => {
    const e = map.get(id);
    return e && e.duration > 0 ? e.position / e.duration : null;
  });
}

/** Seconds to resume from, or 0. */
export async function resumeAt(id: string): Promise<number> {
  await load();
  return map.get(id)?.position ?? 0;
}

export function saveProgress(id: string, position: number, duration: number) {
  if (!Number.isFinite(position) || !Number.isFinite(duration)) return;
  const had = map.has(id);
  if (keep(position, duration)) map.set(id, { position, duration });
  else if (had) map.delete(id);
  else return; // nothing stored, nothing to store
  emit();
  invoke("save_progress", { id, position, duration }).catch((e) => console.warn("progress:", e));
}
