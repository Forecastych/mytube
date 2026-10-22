// Feed rules from the Algorithm screen. Edits apply locally at once and are
// saved shortly after the last change; the backend sanitizes and answers with
// the stored rules plus a preview of the feed they produce.
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useSyncExternalStore } from "react";
import { errorText, type Video } from "./api";

export const SOURCES = ["favorites", "subscriptions", "trending", "related"] as const;
export type Source = (typeof SOURCES)[number];
export type Mix = Record<Source, number>;

export type BlockedChannel = { id: string; title: string; avatar: string | null };
export type BlockedVideo = { id: string; title: string };

export type Rules = {
  blockedTopics: string[];
  blockedWords: string[];
  blockedChannels: BlockedChannel[];
  /** Oldest first. */
  blockedVideos: BlockedVideo[];
  mix: Mix;
  hideShorts: boolean;
  /** 0 = off */
  minMinutes: number;
  hideWatched: boolean;
  watchAffects: boolean;
};

export type Topic = { id: string; label: string };

export type FeedVideo = Video & {
  /** seconds; null for RSS-only videos */
  duration: number | null;
  category: number | null;
  isShort: boolean;
  source: Source;
};

export type Ranked = { videos: FeedVideo[]; hidden: number };

type View = { rules: Rules; topics: Topic[]; preview: Ranked };

const cmd = {
  get: () => invoke<View>("algorithm_get"),
  set: (rules: Rules) => invoke<View>("algorithm_set", { rules }),
  resolveChannel: (input: string) => invoke<BlockedChannel>("blocked_channel_resolve", { input }),
};

type State = {
  rules: Rules | null;
  topics: Topic[];
  preview: Ranked;
  saving: boolean;
  error: string | null;
};

let state: State = { rules: null, topics: [], preview: { videos: [], hidden: 0 }, saving: false, error: null };
const listeners = new Set<() => void>();

function set(patch: Partial<State>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

let loading: Promise<void> | null = null;
function load() {
  if (state.rules) return Promise.resolve();
  loading ??= cmd
    .get()
    .then(
      (v) => set({ rules: v.rules, topics: v.topics, preview: v.preview, error: null }),
      (e) => set({ error: errorText(e) }),
    )
    .finally(() => (loading = null));
  return loading;
}

const SAVE_DELAY_MS = 400;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
/** Bumped on every local edit: a save answer for older rules must not overwrite newer ones. */
let edits = 0;

async function save() {
  const sent = edits;
  const rules = state.rules;
  if (!rules) return;
  set({ saving: true });
  try {
    const v = await cmd.set(rules);
    set(sent === edits ? { rules: v.rules, preview: v.preview, error: null } : { preview: v.preview });
  } catch (e) {
    set({ error: errorText(e) });
  } finally {
    set({ saving: false });
  }
}

export function updateRules(patch: Partial<Rules>) {
  if (!state.rules) return;
  edits++;
  set({ rules: { ...state.rules, ...patch } });
  clearTimeout(saveTimer);
  saveTimer = setTimeout(save, SAVE_DELAY_MS);
}

/** Resolve a pasted channel link and block it. Throws a user-readable string. */
export async function blockChannel(input: string) {
  const ch = await cmd.resolveChannel(input);
  const list = state.rules?.blockedChannels ?? [];
  if (list.some((c) => c.id === ch.id)) throw `${ch.title} is already blocked`;
  updateRules({ blockedChannels: [...list, ch] });
  return ch;
}

/** Rules loaded on demand, for callers outside the Algorithm screen. Throws a user-readable string. */
async function loadedRules() {
  await load();
  if (!state.rules) throw state.error ?? "Couldn't load the feed rules";
  return state.rules;
}

/** Block or unblock a channel whose id is already known (the card menu). */
export async function setChannelBlocked(ch: BlockedChannel, blocked: boolean) {
  const list = (await loadedRules()).blockedChannels.filter((c) => c.id !== ch.id);
  updateRules({ blockedChannels: blocked ? [...list, ch] : list });
}

/** False until the backend knows about blocked videos. */
export async function canBlockVideos() {
  return (await loadedRules().catch(() => null))?.blockedVideos != null;
}

export async function setVideoBlocked(v: BlockedVideo, blocked: boolean) {
  const rules = await loadedRules();
  if (!rules.blockedVideos) throw "Blocking single videos isn't available yet";
  const list = rules.blockedVideos.filter((x) => x.id !== v.id);
  updateRules({ blockedVideos: blocked ? [...list, v] : list });
}

export function useAlgorithm() {
  const s = useSyncExternalStore(subscribe, () => state);
  useEffect(() => {
    void load();
  }, []);
  return s;
}

/**
 * Set one share and spread the rest over the others in proportion to their
 * current values, so the mix always sums to 100.
 */
export function rebalance(mix: Mix, key: Source, value: number): Mix {
  const v = Math.max(0, Math.min(100, Math.round(value)));
  const others = SOURCES.filter((s) => s !== key);
  const rest = 100 - v;
  const sum = others.reduce((a, s) => a + mix[s], 0);
  const raw = others.map((s) => (sum > 0 ? (mix[s] * rest) / sum : rest / others.length));
  const out = raw.map(Math.floor);
  let left = rest - out.reduce((a, x) => a + x, 0);
  // Largest remainder; stable, so ties go to the earlier source.
  const order = raw.map((x, i) => [x - Math.floor(x), i] as const).sort((a, b) => b[0] - a[0]);
  for (const [, i] of order) {
    if (left <= 0) break;
    out[i]++;
    left--;
  }
  const next = { ...mix, [key]: v };
  others.forEach((s, i) => (next[s] = out[i]));
  return next;
}
