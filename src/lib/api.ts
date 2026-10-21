import { invoke } from "@tauri-apps/api/core";

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

export const api = {
  favoritesList: () => invoke<Channel[]>("favorites_list"),
  favoriteAdd: (input: string) => invoke<Channel>("favorite_add", { input }),
  favoriteRemove: (id: string) => invoke<void>("favorite_remove", { id }),
  favoritesFeed: (force = false) => invoke<FavoritesFeed>("favorites_feed", { force }),
};

/** 320×180 — the right size for cards; never maxresdefault. */
export const thumb = (id: string) => `https://i.ytimg.com/vi/${id}/mqdefault.jpg`;

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong";
}
