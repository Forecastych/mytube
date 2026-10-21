// Glass controls over the video. All playback state lives here, local: the
// rest of the page never re-renders on `timeupdate`.
import { memo, useEffect, useRef, useState, type RefObject } from "react";
import {
  Check,
  ExternalLink,
  LoaderCircle,
  Maximize,
  Minimize,
  Pause,
  Play,
  Settings,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import type { SponsorSegment, Track, WatchInfo } from "../../lib/api";
import { pref, setPref } from "../../lib/prefs";
import { clock } from "../../lib/format";
import { qualityLabel } from "./Player";

const HIDE_AFTER = 2500;
const RATES = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];

const CATEGORY: Record<string, string> = {
  sponsor: "sponsor",
  selfpromo: "self-promotion",
  interaction: "subscribe reminder",
  intro: "intro",
  outro: "outro",
  preview: "preview",
  music_offtopic: "non-music part",
  filler: "filler",
};

type Props = {
  video: RefObject<HTMLVideoElement | null>;
  box: RefObject<HTMLDivElement | null>;
  info: WatchInfo;
  qualities: Track[];
  quality: Track | undefined;
  onQuality: (t: Track) => void;
  sponsor: SponsorSegment[];
  fullscreen: boolean;
  onFullscreen: () => void;
  onMpv: () => void;
};

function bufferedAhead(v: HTMLVideoElement) {
  const b = v.buffered;
  for (let i = 0; i < b.length; i++) if (b.start(i) <= v.currentTime + 0.1 && v.currentTime <= b.end(i)) return b.end(i);
  return v.currentTime;
}

export const Controls = memo(function Controls(p: Props) {
  const { video, box, info, sponsor } = p;
  const [playing, setPlaying] = useState(false);
  const [waiting, setWaiting] = useState(true);
  const [time, setTime] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [duration, setDuration] = useState(info.duration);
  const [volume, setVolume] = useState(pref("volume"));
  const [muted, setMuted] = useState(pref("muted"));
  const [rate, setRate] = useState(pref("rate"));
  const [menu, setMenu] = useState(false);
  const [shown, setShown] = useState(true);
  const [hover, setHover] = useState<number | null>(null);
  const [toast, setToast] = useState<{ seg: SponsorSegment } | null>(null);
  const [scrub, setScrub] = useState<number | null>(null);
  const seekRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef(0);
  const skipped = useRef(new Set<SponsorSegment>());
  const toastTimer = useRef(0);

  // Mirror the element's state.
  useEffect(() => {
    const v = video.current!;
    const skip = new Set(pref("skipCategories"));
    const onTime = () => {
      setTime(v.currentTime);
      setBuffered(bufferedAhead(v));
      // SponsorBlock: jump over a segment once; seeking back into it plays it.
      for (const s of sponsor) {
        if (!skip.has(s.category) || skipped.current.has(s)) continue;
        if (v.currentTime >= s.start && v.currentTime < s.end - 0.5) {
          skipped.current.add(s);
          v.currentTime = s.end;
          setToast({ seg: s });
          clearTimeout(toastTimer.current);
          toastTimer.current = window.setTimeout(() => setToast(null), 5000);
          break;
        }
      }
    };
    const on: Record<string, () => void> = {
      play: () => setPlaying(true),
      pause: () => {
        setPlaying(false);
        setShown(true);
      },
      ended: () => setPlaying(false),
      waiting: () => setWaiting(true),
      playing: () => setWaiting(false),
      canplay: () => setWaiting(false),
      seeking: onTime,
      timeupdate: onTime,
      progress: () => setBuffered(bufferedAhead(v)),
      durationchange: () => Number.isFinite(v.duration) && setDuration(v.duration),
      volumechange: () => {
        setVolume(v.volume);
        setMuted(v.muted);
        setPref("volume", v.volume);
        setPref("muted", v.muted);
      },
      ratechange: () => {
        setRate(v.playbackRate);
        setPref("rate", v.playbackRate);
      },
    };
    for (const [k, f] of Object.entries(on)) v.addEventListener(k, f);
    return () => {
      for (const [k, f] of Object.entries(on)) v.removeEventListener(k, f);
      clearTimeout(toastTimer.current);
    };
  }, [video, sponsor]);

  // Auto-hide: timers run only after the mouse moved, never while idle.
  useEffect(() => {
    const el = box.current!;
    const poke = () => {
      setShown(true);
      clearTimeout(hideTimer.current);
      hideTimer.current = window.setTimeout(() => {
        if (!video.current?.paused) setShown(false);
      }, HIDE_AFTER);
    };
    const leave = () => !video.current?.paused && setShown(false);
    el.addEventListener("pointermove", poke);
    el.addEventListener("pointerleave", leave);
    return () => {
      el.removeEventListener("pointermove", poke);
      el.removeEventListener("pointerleave", leave);
      clearTimeout(hideTimer.current);
    };
  }, [box, video]);

  const toggle = () => {
    const v = video.current!;
    if (v.paused || v.ended) void v.play().catch(() => {});
    else v.pause();
  };
  const seekBy = (d: number) => {
    const v = video.current!;
    v.currentTime = Math.min(Math.max(0, v.currentTime + d), duration || v.duration);
  };

  // Keyboard, YouTube-style. Ignored while typing in a field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.isContentEditable || t.tagName === "INPUT" || t.tagName === "TEXTAREA" || e.ctrlKey || e.altKey || e.metaKey)
        return;
      const v = video.current!;
      const k = e.key.toLowerCase();
      const act: Record<string, () => void> = {
        " ": toggle,
        k: toggle,
        j: () => seekBy(-10),
        l: () => seekBy(10),
        arrowleft: () => seekBy(-5),
        arrowright: () => seekBy(5),
        arrowup: () => (v.volume = Math.min(1, v.volume + 0.05)),
        arrowdown: () => (v.volume = Math.max(0, v.volume - 0.05)),
        m: () => (v.muted = !v.muted),
        f: p.onFullscreen,
        escape: () => p.fullscreen && p.onFullscreen(),
        home: () => (v.currentTime = 0),
        end: () => (v.currentTime = duration),
      };
      if (/^[0-9]$/.test(k)) act[k] = () => (v.currentTime = (duration * Number(k)) / 10);
      const f = act[k];
      if (!f) return;
      e.preventDefault();
      f();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const frac = (clientX: number) => {
    const r = seekRef.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width));
  };
  const onSeekDown = (e: React.PointerEvent) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setScrub(frac(e.clientX) * duration);
  };
  const onSeekMove = (e: React.PointerEvent) => {
    const t = frac(e.clientX) * duration;
    setHover(t);
    if (scrub != null) setScrub(t);
  };
  const onSeekUp = () => {
    if (scrub != null) video.current!.currentTime = scrub;
    setScrub(null);
  };

  const shownTime = scrub ?? time;
  const pct = (t: number) => `${duration ? (t / duration) * 100 : 0}%`;
  const chapterAt = (t: number) => info.chapters.find((c) => t >= c.startTime && t < c.endTime)?.title;
  const VolIcon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  const visible = shown || menu || !playing;

  return (
    <>
      <div className="player-hit" onClick={toggle} onDoubleClick={p.onFullscreen} style={visible ? undefined : { cursor: "none" }} />
      {waiting && (
        <div className="player-center">
          <LoaderCircle size={40} className="spin" />
        </div>
      )}
      {toast && (
        <div className="player-toast glass">
          Skipped {CATEGORY[toast.seg.category] ?? toast.seg.category}
          <button
            onClick={() => {
              video.current!.currentTime = toast.seg.start;
              setToast(null);
            }}
          >
            Undo
          </button>
        </div>
      )}

      <div className={`player-bar glass${visible ? "" : " hidden"}`}>
        <div
          ref={seekRef}
          className="seek"
          onPointerDown={onSeekDown}
          onPointerMove={onSeekMove}
          onPointerUp={onSeekUp}
          onPointerLeave={() => setHover(null)}
        >
          <div className="seek-track">
            <div className="seek-buffered" style={{ width: pct(buffered) }} />
            {sponsor.map((s, i) => (
              <div key={i} className="seek-sponsor" style={{ left: pct(s.start), width: pct(s.end - s.start) }} />
            ))}
            {info.chapters.slice(1).map((c) => (
              <div key={c.startTime} className="seek-chapter" style={{ left: pct(c.startTime) }} />
            ))}
            <div className="seek-played" style={{ width: pct(shownTime) }} />
          </div>
          <div className="seek-thumb" style={{ left: pct(shownTime) }} />
          {hover != null && (
            <div className="seek-tip glass" style={{ left: pct(hover) }}>
              {chapterAt(hover) && <span>{chapterAt(hover)}</span>}
              {clock(hover)}
            </div>
          )}
        </div>

        <div className="player-row">
          <button className="pbtn" onClick={toggle} title={playing ? "Pause (k)" : "Play (k)"}>
            {playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
          </button>
          <div className="volume">
            <button className="pbtn" onClick={() => (video.current!.muted = !muted)} title="Mute (m)">
              <VolIcon size={22} />
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.02}
              value={muted ? 0 : volume}
              onChange={(e) => {
                const v = video.current!;
                v.volume = Number(e.target.value);
                v.muted = v.volume === 0;
              }}
              aria-label="Volume"
            />
          </div>
          <span className="player-time">
            {clock(shownTime)} / {clock(duration)}
          </span>
          {chapterAt(shownTime) && <span className="player-chapter">· {chapterAt(shownTime)}</span>}

          <span className="spacer" />
          <div className="menu-anchor">
            {menu && (
              <div className="player-menu glass" onPointerLeave={() => setMenu(false)}>
                <div className="menu-title">Quality</div>
                {p.qualities.map((q) => (
                  <button
                    key={q.key}
                    onClick={() => {
                      p.onQuality(q);
                      setMenu(false);
                    }}
                  >
                    {q.key === p.quality?.key ? <Check size={14} /> : <span className="menu-gap" />}
                    {qualityLabel(q)}
                    {q.codec.startsWith("av01") && <span className="muted small">AV1</span>}
                  </button>
                ))}
                {p.qualities.length === 0 && <div className="muted small menu-note">360p (fallback)</div>}
                <div className="menu-title">Speed</div>
                <div className="menu-rates">
                  {RATES.map((r) => (
                    <button
                      key={r}
                      className={r === rate ? "active" : undefined}
                      onClick={() => (video.current!.playbackRate = r)}
                    >
                      {r}×
                    </button>
                  ))}
                </div>
              </div>
            )}
            <button className="pbtn pill" onClick={() => setMenu(!menu)} title="Quality and speed">
              {p.quality ? qualityLabel(p.quality) : "Auto"}
              {rate !== 1 && ` · ${rate}×`}
              <Settings size={16} />
            </button>
          </div>
          <button className="pbtn" onClick={p.onMpv} title="Open in mpv (4K, HDR)">
            <ExternalLink size={20} />
          </button>
          <button className="pbtn" onClick={p.onFullscreen} title="Fullscreen (f)">
            {p.fullscreen ? <Minimize size={20} /> : <Maximize size={20} />}
          </button>
        </div>
      </div>
    </>
  );
});
