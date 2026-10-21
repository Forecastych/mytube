import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { TrackIndex } from "./player/engine";

export type Channel = {
  id: string;
  title: string;
  avatar: string | null;
  newCount: number;
};

export type Video = {
  id: string;
  channelId: string;
  channelTitle: string;
  channelAvatar: string | null;
  title: string;
  /** unix seconds */
  published: number;
  views: number | null;
  isNew: boolean;
};

export type FavoritesFeed = { videos: Video[]; failed: string[] };

export type Track = {
  /** path under `stream://` */
  key: string;
  codec: string;
  mime: string;
  height: number;
  fps: number;
  bitrate: number;
};

export type Chapter = { startTime: number; endTime: number; title: string };

export type WatchInfo = {
  id: string;
  title: string;
  channel: string;
  channelId: string | null;
  description: string;
  duration: number;
  views: number | null;
  likes: number | null;
  /** unix seconds */
  published: number | null;
  isLive: boolean;
  chapters: Chapter[];
  video: Track[];
  audio: Track | null;
  progressive: Track | null;
  isFavorite: boolean;
};

export type SponsorSegment = { category: string; start: number; end: number };

export const api = {
  favoritesList: () => invoke<Channel[]>("favorites_list"),
  favoriteAdd: (input: string) => invoke<Channel>("favorite_add", { input }),
  favoriteRemove: (id: string) => invoke<void>("favorite_remove", { id }),
  favoritesFeed: (force = false) => invoke<FavoritesFeed>("favorites_feed", { force }),
  watchInfo: (id: string, force = false) => invoke<WatchInfo>("watch_info", { id, force }),
  streamIndex: (key: string) => invoke<TrackIndex>("stream_index", { key }),
  sponsorSegments: (id: string) => invoke<SponsorSegment[]>("sponsor_segments", { id }),
  openInMpv: (id: string, title: string, start: number) => invoke<void>("open_in_mpv", { id, title, start }),
  markWatched: (id: string) => invoke<void>("mark_watched", { id }),
};

/** `stream://` URL the webview can fetch (`http://stream.localhost/...` on Windows). */
export const streamUrl = (key: string) => convertFileSrc(key, "stream");

/** 320×180 — the right size for cards; never maxresdefault. */
export const thumb = (id: string) => `https://i.ytimg.com/vi/${id}/mqdefault.jpg`;
/** 480×360 poster while the player loads. */
export const poster = (id: string) => `https://i.ytimg.com/vi/${id}/hqdefault.jpg`;

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong";
}
