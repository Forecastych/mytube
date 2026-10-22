// Videos and channels blocked from a card's context menu. The backend filters
// them out of the next feed it builds; this store hides them from what's on
// screen right now, so a block takes effect without a refetch.
import { useSyncExternalStore } from "react";

const videos = new Set<string>();
const channels = new Set<string>();
const listeners = new Set<() => void>();

function changed() {
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function hide(kind: "video" | "channel", id: string, on = true) {
  const set = kind === "video" ? videos : channels;
  if (on === set.has(id)) return;
  if (on) set.add(id);
  else set.delete(id);
  changed();
}

export const isHidden = (videoId: string, channelId: string) => videos.has(videoId) || channels.has(channelId);

/** Re-renders only when this card's answer flips. */
export function useHidden(videoId: string, channelId: string) {
  return useSyncExternalStore(subscribe, () => isHidden(videoId, channelId));
}
