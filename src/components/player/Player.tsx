import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ExternalLink, LoaderCircle, RotateCw } from "lucide-react";
import { api, errorText, poster, streamUrl, type SponsorSegment, type Track, type WatchInfo } from "../../lib/api";
import { Engine, StreamError, type TrackSource } from "../../lib/player/engine";
import { markWatchedLocal } from "../../lib/favorites";
import { clock } from "../../lib/format";
import { pref, setPref } from "../../lib/prefs";
import { saveProgress } from "../../lib/progress";
import { Controls } from "./Controls";
import "./Player.css";

const typeOf = (t: Track) => `${t.mime}; codecs="${t.codec}"`;

/** One entry per height/fps the webview can decode; H.264 beats AV1 (hardware decode everywhere). */
export function playableQualities(info: WatchInfo): Track[] {
  const out = new Map<string, Track>();
  for (const t of info.video) {
    const k = `${t.height}p${t.fps > 30 ? t.fps : ""}`;
    const prev = out.get(k);
    if (!Engine.supported(typeOf(t))) continue;
    if (!prev || (prev.codec.startsWith("av01") && t.codec.startsWith("avc1"))) out.set(k, t);
  }
  return [...out.values()];
}

export const qualityLabel = (t: Track) => `${t.height}p${t.fps > 30 ? t.fps : ""}`;

function pickQuality(list: Track[]): Track | undefined {
  const want = pref("quality");
  return list.find((t) => t.height <= want) ?? list[list.length - 1];
}

async function source(t: Track): Promise<TrackSource> {
  return { url: streamUrl(t.key), type: typeOf(t), index: await api.streamIndex(t.key) };
}

type Props = {
  info: WatchInfo;
  startAt: number;
  /** Stream URLs expired (403) or the user hit Retry: refetch info and continue from `t`. */
  onExpired: (t: number, manual?: boolean) => void;
  /** The page reads/seeks the element through this (timestamps in the description). */
  videoRef?: RefObject<HTMLVideoElement | null>;
  /** `startAt` came from saved progress: offer "Start over". */
  resumed?: boolean;
};

export function Player({ info, startAt, onExpired, videoRef: outer, resumed }: Props) {
  const own = useRef<HTMLVideoElement>(null);
  const videoRef = outer ?? own;
  const boxRef = useRef<HTMLDivElement>(null);
  const engineRef = useRef<Engine | null>(null);
  const qualities = useMemo(() => playableQualities(info), [info]);
  const [quality, setQuality] = useState<Track | undefined>(() => pickQuality(qualities));
  const [error, setError] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [sponsor, setSponsor] = useState<SponsorSegment[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [resumeHint, setResumeHint] = useState(!!resumed);

  useEffect(() => {
    if (!resumeHint) return;
    const t = setTimeout(() => setResumeHint(false), 8000);
    return () => clearTimeout(t);
  }, [resumeHint]);

  // Attach the stream. Re-runs only for a new video, not for quality changes.
  useEffect(() => {
    const video = videoRef.current!;
    video.volume = pref("volume");
    video.muted = pref("muted");
    video.playbackRate = pref("rate");
    let cancelled = false;
    let played = false;
    const markPlayed = () => (played = true);
    video.addEventListener("playing", markPlayed, { once: true });

    const progressive = () => {
      if (!info.progressive) return false;
      engineRef.current?.destroy();
      engineRef.current = null;
      video.src = streamUrl(info.progressive.key);
      video.currentTime = startAt;
      void video.play().catch(() => {});
      return true;
    };

    const onEngineError = (e: Error) => {
      if (cancelled) return;
      if (e instanceof StreamError && e.status === 403) return onExpired(video.currentTime);
      console.error("player:", e);
      // MSE choked before anything played: the muxed 360p stream may still work.
      if (!played && progressive()) return;
      setError(e.message);
    };

    (async () => {
      const q = pickQuality(qualities);
      if (!q || !info.audio || !Engine.supported(typeOf(info.audio))) {
        if (!progressive()) setError("No playable stream for this video. Try Open in mpv.");
        return;
      }
      try {
        const [v, a] = await Promise.all([source(q), source(info.audio)]);
        if (cancelled) return;
        engineRef.current = new Engine(video, [v, a], startAt, onEngineError);
        void video.play().catch(() => {});
      } catch (e) {
        if (!cancelled && !progressive()) setError(errorText(e));
      }
    })();

    return () => {
      cancelled = true;
      video.removeEventListener("playing", markPlayed);
      engineRef.current?.destroy();
      engineRef.current = null;
    };
    // `startAt`/`onExpired` belong to the same load; a new info object means a new load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info, qualities]);

  const changeQuality = useCallback(
    async (t: Track) => {
      setQuality(t);
      setPref("quality", t.height);
      const engine = engineRef.current;
      if (!engine) return;
      try {
        await engine.switchVideo(await source(t));
      } catch (e) {
        setError(errorText(e));
      }
    },
    [],
  );

  // SponsorBlock: fetched once per video, skipped in Controls' time loop.
  useEffect(() => {
    if (!pref("sponsorBlock") || info.isLive) return;
    let alive = true;
    api.sponsorSegments(info.id).then(
      (s) => alive && setSponsor(s),
      (e) => console.warn("sponsorblock:", errorText(e)),
    );
    return () => {
      alive = false;
    };
  }, [info]);

  // Counts as watched after 30 s or half the video, whichever comes first.
  useEffect(() => {
    const video = videoRef.current!;
    const threshold = Math.min(30, (info.duration || 60) / 2);
    const onTime = () => {
      if (video.currentTime < threshold) return;
      video.removeEventListener("timeupdate", onTime);
      api.markWatched(info.id).then(() => markWatchedLocal(info.id), console.warn);
    };
    video.addEventListener("timeupdate", onTime);
    return () => video.removeEventListener("timeupdate", onTime);
  }, [info]);

  // Remember the position: on pause, every 10 s of playback and on leaving.
  useEffect(() => {
    if (info.isLive) return;
    const video = videoRef.current!;
    const length = () => info.duration || video.duration;
    // Tracked here: by cleanup time the engine may already have reset the element.
    // Nothing counts until playback starts: a page opened and closed keeps its old position.
    let pos = -1;
    let saved = -1;
    let started = false;
    const save = () => {
      if (pos >= 0 && pos !== saved) saveProgress(info.id, pos, length());
      saved = pos;
    };
    const onPlaying = () => (started = true);
    const onTime = () => {
      if (!started || video.seeking) return;
      pos = video.currentTime;
      if (Math.abs(pos - saved) >= 10) save();
    };
    const onEnded = () => {
      pos = length();
      save();
    };
    video.addEventListener("playing", onPlaying);
    video.addEventListener("pause", save);
    video.addEventListener("timeupdate", onTime);
    video.addEventListener("ended", onEnded);
    return () => {
      save();
      video.removeEventListener("playing", onPlaying);
      video.removeEventListener("pause", save);
      video.removeEventListener("timeupdate", onTime);
      video.removeEventListener("ended", onEnded);
    };
  }, [info]);

  const toggleFullscreen = useCallback(async () => {
    const next = !fullscreen;
    setFullscreen(next);
    try {
      await getCurrentWindow().setFullscreen(next);
    } catch {
      /* plain browser (vite dev): CSS fullscreen only */
    }
  }, [fullscreen]);

  // Leaving the page must not leave the window fullscreen.
  useEffect(() => {
    if (!fullscreen) return;
    return () => void getCurrentWindow().setFullscreen(false).catch(() => {});
  }, [fullscreen]);

  const openMpv = useCallback(() => {
    const video = videoRef.current;
    video?.pause();
    // A missing mpv doesn't break this player: say so and keep it.
    api.openInMpv(info.id, info.title, video?.currentTime ?? 0).catch((e) => setNotice(errorText(e)));
  }, [info]);

  return (
    <div ref={boxRef} className={`player${fullscreen ? " fs" : ""}`}>
      <video ref={videoRef} poster={poster(info.id)} playsInline />
      {error ? (
        <div className="player-overlay">
          <p>{error}</p>
          <div className="player-overlay-actions">
            <button className="btn" onClick={() => onExpired(videoRef.current?.currentTime || startAt, true)}>
              <RotateCw size={16} /> Retry
            </button>
            <button className="btn" onClick={openMpv}>
              <ExternalLink size={16} /> Open in mpv
            </button>
          </div>
        </div>
      ) : (
        <Controls
          video={videoRef}
          box={boxRef}
          info={info}
          qualities={qualities}
          quality={quality}
          onQuality={changeQuality}
          sponsor={sponsor}
          fullscreen={fullscreen}
          onFullscreen={toggleFullscreen}
          onMpv={openMpv}
        />
      )}
      {resumeHint && !error && (
        <div className="player-toast player-resume glass">
          Continuing from {clock(startAt)}
          <button
            onClick={() => {
              videoRef.current!.currentTime = 0;
              setResumeHint(false);
            }}
          >
            Start over
          </button>
        </div>
      )}
      {notice && (
        <div className="player-toast player-notice glass" onClick={() => setNotice(null)}>
          {notice}
        </div>
      )}
    </div>
  );
}

/** Same box as the player, for the seconds yt-dlp takes. */
export function PlayerPlaceholder({ id, children }: { id: string; children?: React.ReactNode }) {
  return (
    <div className="player">
      <img className="player-poster" src={poster(id)} alt="" decoding="async" />
      <div className="player-overlay">
        {children ?? <LoaderCircle size={36} className="spin" />}
      </div>
    </div>
  );
}
