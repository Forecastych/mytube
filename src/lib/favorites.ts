// Favorites + their RSS feed, shared by Home and Favorites. A module-level
// store: switching pages doesn't refetch, and only subscribers re-render.
import { useEffect, useSyncExternalStore } from "react";
import { api, errorText, type Channel, type Video } from "./api";

type State = {
  channels: Channel[];
  videos: Video[];
  failed: string[];
  loading: boolean;
  loaded: boolean;
  error: string | null;
};

let state: State = { channels: [], videos: [], failed: [], loading: false, loaded: false, error: null };
const listeners = new Set<() => void>();

function set(patch: Partial<State>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export async function refresh(force = false) {
  if (state.loading) return;
  set({ loading: true, error: null });
  try {
    // Feed first: it refetches stale RSS, which updates the NEW counters.
    const feed = await api.favoritesFeed(force);
    const channels = await api.favoritesList();
    set({ channels, videos: feed.videos, failed: feed.failed, loaded: true });
  } catch (e) {
    set({ error: errorText(e) });
  } finally {
    set({ loading: false });
  }
}

/** Throws a user-readable string on failure. */
export async function addFavorite(input: string) {
  await api.favoriteAdd(input);
  const [channels, feed] = await Promise.all([api.favoritesList(), api.favoritesFeed(false)]);
  set({ channels, videos: feed.videos, failed: feed.failed });
}

export async function removeFavorite(id: string) {
  await api.favoriteRemove(id);
  set({
    channels: state.channels.filter((c) => c.id !== id),
    videos: state.videos.filter((v) => v.channelId !== id),
  });
}

export function useFavorites() {
  const s = useSyncExternalStore(subscribe, () => state);
  useEffect(() => {
    if (!state.loaded) void refresh();
  }, []);
  return s;
}
