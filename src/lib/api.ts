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

/** `hidden`: dropped by the Algorithm rules. */
export type FavoritesFeed = { videos: Video[]; hidden: number; failed: string[] };

export type SearchChannel = {
  id: string;
  title: string;
  avatar: string | null;
  /** "@kurzgesagt" */
  handle: string | null;
  /** "25.7M subscribers" */
  subscribers: string | null;
};
/** `hidden`: dropped by the Algorithm rules. */
export type SearchPage = { channels: SearchChannel[]; videos: Video[]; hidden: number; next: string | null };

/** `hidden`: dropped by the Algorithm rules (related) or Shorts left out (channel). */
export type Related = { videos: Video[]; hidden: number };

export type ChannelPage = {
  id: string;
  title: string;
  avatar: string | null;
  banner: string | null;
  /** "@kurzgesagt" */
  handle: string | null;
  /** "25.7M subscribers" */
  subscribers: string | null;
  /** "394 videos" */
  videoCount: string | null;
  description: string | null;
  videos: Video[];
  hidden: number;
  isFavorite: boolean;
  isSubscribed: boolean;
  isBlocked: boolean;
};

export type Track = {
  /** path under `stream://` */
  key: string;
  codec: string;
  mime: string;
  height: number;
  fps: number;
  bitrate: number;
  /** Audio language, e.g. `en-US`; null on videos with one track. */
  lang: string | null;
  /** "English (United States)" */
  label: string | null;
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
  /** One per language, the original first; empty when there's no DASH audio. */
  audio: Track[];
  progressive: Track | null;
  isFavorite: boolean;
};

export type Comment = {
  id: string;
  author: string;
  avatar: string | null;
  text: string;
  /** "3 days ago (edited)" */
  published: string;
  likes: string | null;
  replyCount: string | null;
  /** token for the first page of replies */
  replies: string | null;
  pinned: boolean;
  verified: boolean;
  /** written by the video's channel */
  creator: boolean;
};

export type CommentsPage = {
  count: string | null;
  sort: { title: string; token: string }[];
  comments: Comment[];
  next: string | null;
  disabled: boolean;
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
  /** `audio`: key of the audio track to play instead of the original. */
  openInMpv: (id: string, title: string, start: number, audio?: string) =>
    invoke<void>("open_in_mpv", { id, title, start, audio }),
  comments: (id: string) => invoke<CommentsPage>("comments", { id }),
  /** Next page, another sort order, or a thread's replies. */
  commentsMore: (token: string) => invoke<CommentsPage>("comments_more", { token }),
  /** Anonymous Innertube search; `next` continues the same query. */
  search: (query: string, next: string | null = null) => invoke<SearchPage>("search", { query, next }),
  /** Videos YouTube shows next to this one, through the Algorithm rules. */
  related: (id: string) => invoke<Related>("related", { id }),
  channelPage: (id: string) => invoke<ChannelPage>("channel_page", { id }),
  markWatched: (id: string) => invoke<void>("mark_watched", { id }),
  ytdlpVersion: () => invoke<string>("ytdlp_version"),
  /** Runs `yt-dlp -U`; resolves with its last line of output. */
  ytdlpUpdate: () => invoke<string>("ytdlp_update"),
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
