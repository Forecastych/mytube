// Small per-machine preferences in localStorage. Settings will edit these.
// Reads never throw: a missing or broken value falls back to the default.

export type Prefs = {
  volume: number;
  muted: boolean;
  /** Preferred video height; the closest available one is picked. */
  quality: number;
  rate: number;
  sponsorBlock: boolean;
  /** SponsorBlock categories skipped without asking. */
  skipCategories: string[];
  upNextOpen: boolean;
};

const defaults: Prefs = {
  volume: 1,
  muted: false,
  quality: 1080,
  rate: 1,
  sponsorBlock: true,
  skipCategories: ["sponsor", "selfpromo", "interaction"],
  upNextOpen: true,
};

export function pref<K extends keyof Prefs>(key: K): Prefs[K] {
  try {
    const raw = localStorage.getItem(`pref.${key}`);
    return raw == null ? defaults[key] : (JSON.parse(raw) as Prefs[K]);
  } catch {
    return defaults[key];
  }
}

export function setPref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
  try {
    localStorage.setItem(`pref.${key}`, JSON.stringify(value));
  } catch {
    /* storage full or blocked: the setting just won't stick */
  }
}
