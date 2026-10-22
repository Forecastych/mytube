// Small per-machine preferences in localStorage, edited on the Settings screen.
// Reads never throw: a missing or broken value falls back to the default.

import { useSyncExternalStore } from "react";

export type Theme = "dark" | "light";

export type Prefs = {
  volume: number;
  muted: boolean;
  /** Preferred video height; the closest available one is picked. */
  quality: number;
  rate: number;
  /** Preferred dub language, e.g. `de-DE`; empty = the original audio. */
  audioLang: string;
  sponsorBlock: boolean;
  /** SponsorBlock categories skipped without asking. */
  skipCategories: string[];
  upNextOpen: boolean;
  /** What Up next lists: videos like this one, or new uploads of favorites. */
  upNextSource: "related" | "favorites";
  theme: Theme;
  /** Same defaults as tokens.css. */
  glassAlpha: number;
  glassBlur: number;
  reduceTransparency: boolean;
};

export const defaults: Prefs = {
  volume: 1,
  muted: false,
  quality: 1080,
  rate: 1,
  audioLang: "",
  sponsorBlock: true,
  skipCategories: ["sponsor", "selfpromo", "interaction"],
  upNextOpen: true,
  upNextSource: "related",
  theme: "dark",
  glassAlpha: 0.55,
  glassBlur: 18,
  reduceTransparency: false,
};

type Key = keyof Prefs;

const listeners = new Set<(key: Key) => void>();
// useSyncExternalStore needs a stable value between changes: arrays from
// JSON.parse are new every time, so reads are cached until the next set.
const cache = new Map<Key, unknown>();

export function pref<K extends Key>(key: K): Prefs[K] {
  if (cache.has(key)) return cache.get(key) as Prefs[K];
  let value = defaults[key];
  try {
    const raw = localStorage.getItem(`pref.${key}`);
    if (raw != null) value = JSON.parse(raw) as Prefs[K];
  } catch {
    /* broken value: default */
  }
  cache.set(key, value);
  return value;
}

export function setPref<K extends Key>(key: K, value: Prefs[K]) {
  cache.set(key, value);
  try {
    localStorage.setItem(`pref.${key}`, JSON.stringify(value));
  } catch {
    /* storage full or blocked: the setting holds until restart */
  }
  listeners.forEach((l) => l(key));
}

export function onPrefChange(fn: (key: Key) => void) {
  listeners.add(fn);
  return () => void listeners.delete(fn);
}

/** A pref that re-renders the component when anyone changes it. */
export function usePref<K extends Key>(key: K): [Prefs[K], (v: Prefs[K]) => void] {
  const value = useSyncExternalStore(
    (notify) => onPrefChange((k) => k === key && notify()),
    () => pref(key),
  );
  return [value, (v) => setPref(key, v)];
}
