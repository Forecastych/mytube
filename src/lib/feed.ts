// Home's mixed feed and Trending. Both are ranked in Rust by the Algorithm
// rules; this only calls and caches per screen, so going back is instant.
import { invoke } from "@tauri-apps/api/core";
import type { Video } from "./api";

export type Source = "favorites" | "subscriptions" | "trending" | "related";

export type HomeFeed = {
  videos: Video[];
  hidden: number;
  signedIn: boolean;
  /** Subscribed channels imported so far. */
  subscriptions: number;
  failed: string[];
  /** A source that couldn't load; the rest still shows. */
  notice: string | null;
};

export type Chart = { videos: Video[]; hidden: number };

/** The system's country (`ru-RU` → `RU`), else Global. */
export function guessRegion() {
  const r = navigator.language.split("-")[1]?.toUpperCase();
  return r && /^[A-Z]{2}$/.test(r) ? r : "GLOBAL";
}

const homes = new Map<string, HomeFeed>();
export const cachedHome = (source: Source | null) => homes.get(source ?? "all");

export async function homeFeed(source: Source | null) {
  const feed = await invoke<HomeFeed>("home_feed", { source, region: guessRegion() });
  homes.set(source ?? "all", feed);
  return feed;
}

const charts = new Map<string, Chart>();
export const chartKey = (region: string, category: number | null) => `${region}:${category ?? 0}`;
export const cachedChart = (region: string, category: number | null) => charts.get(chartKey(region, category));

export async function trending(region: string, category: number | null) {
  const chart = await invoke<Chart>("trending", { region, category });
  charts.set(chartKey(region, category), chart);
  return chart;
}

/** Re-imports the subscription list; returns how many channels. */
export async function syncSubscriptions() {
  const n = await invoke<number>("subscriptions_sync");
  homes.clear();
  return n;
}
