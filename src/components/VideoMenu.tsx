// Right-click menu for video cards, plus the "Blocked … · Undo" toast it
// leaves behind. One instance lives in App; cards call `openVideoMenu`.
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type MouseEvent } from "react";
import { Ban, EyeOff, Link2, MonitorPlay, Star, StarOff } from "lucide-react";
import { api, errorText, type Video } from "../lib/api";
import { canBlockVideos, setChannelBlocked, setVideoBlocked } from "../lib/algorithm";
import { addFavorite, removeFavorite, useFavorites } from "../lib/favorites";
import { hide } from "../lib/hidden";
import "./VideoMenu.css";

type Open = { v: Video; x: number; y: number };
type Toast = { text: string; undo?: () => void; error?: boolean };

let open: Open | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function openVideoMenu(e: MouseEvent, v: Video) {
  e.preventDefault();
  open = { v, x: e.clientX, y: e.clientY };
  emit();
}

const close = () => {
  if (!open) return;
  open = null;
  emit();
};

const TOAST_MS = 6000;
const GAP = 8;

export function VideoMenu() {
  const menu = useSyncExternalStore(subscribe, () => open);
  const [toast, setToast] = useState<Toast | null>(null);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), TOAST_MS);
    return () => clearTimeout(t);
  }, [toast]);

  return (
    <>
      {menu && <Menu key={`${menu.v.id}:${menu.x}:${menu.y}`} {...menu} toast={setToast} />}
      {toast && (
        <div className={`toast glass${toast.error ? " error" : ""}`} role="status">
          <span>{toast.text}</span>
          {toast.undo && (
            <button
              className="toast-undo"
              onClick={() => {
                toast.undo!();
                setToast(null);
              }}
            >
              Undo
            </button>
          )}
        </div>
      )}
    </>
  );
}

function Menu({ v, x, y, toast }: Open & { toast: (t: Toast) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  const [videoOk, setVideoOk] = useState(false);
  const { channels } = useFavorites();
  const favorite = channels.some((c) => c.id === v.channelId);

  // Keep the menu on screen: flip left/up when it would spill over an edge.
  useLayoutEffect(() => {
    const r = ref.current!.getBoundingClientRect();
    const left = x + r.width + GAP > innerWidth ? Math.max(GAP, x - r.width) : x;
    const top = y + r.height + GAP > innerHeight ? Math.max(GAP, y - r.height) : y;
    setPos({ left, top });
  }, [x, y]);

  useEffect(() => {
    let alive = true;
    canBlockVideos().then((ok) => alive && setVideoOk(ok));
    const outside = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && close();
    const key = (e: KeyboardEvent) => e.key === "Escape" && close();
    addEventListener("pointerdown", outside, true);
    addEventListener("keydown", key);
    addEventListener("wheel", close, { passive: true });
    addEventListener("resize", close);
    addEventListener("blur", close);
    addEventListener("hashchange", close);
    return () => {
      alive = false;
      removeEventListener("pointerdown", outside, true);
      removeEventListener("keydown", key);
      removeEventListener("wheel", close);
      removeEventListener("resize", close);
      removeEventListener("blur", close);
      removeEventListener("hashchange", close);
    };
  }, []);

  const run = (fn: () => Promise<unknown>) => () => {
    close();
    fn().catch((e) => toast({ text: errorText(e), error: true }));
  };

  const blockChannel = run(async () => {
    const ch = { id: v.channelId, title: v.channelTitle, avatar: v.channelAvatar };
    await setChannelBlocked(ch, true);
    hide("channel", ch.id);
    toast({
      text: `Blocked ${ch.title}`,
      undo: () => {
        hide("channel", ch.id, false);
        setChannelBlocked(ch, false).catch(() => {});
      },
    });
  });

  const blockVideo = run(async () => {
    const bv = { id: v.id, title: v.title };
    await setVideoBlocked(bv, true);
    hide("video", bv.id);
    toast({
      text: "Video hidden",
      undo: () => {
        hide("video", bv.id, false);
        setVideoBlocked(bv, false).catch(() => {});
      },
    });
  });

  const toggleFavorite = run(async () => {
    if (favorite) await removeFavorite(v.channelId);
    else await addFavorite(v.channelId);
    toast({ text: favorite ? `${v.channelTitle} removed from favorites` : `${v.channelTitle} added to favorites` });
  });

  const copy = run(async () => {
    await navigator.clipboard.writeText(`https://youtu.be/${v.id}`);
    toast({ text: "Link copied" });
  });

  const mpv = run(() => api.openInMpv(v.id, v.title, 0));

  return (
    <div
      ref={ref}
      className="vmenu glass"
      role="menu"
      style={pos}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
        e.preventDefault();
        const items = [...ref.current!.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        const step = e.key === "ArrowDown" ? 1 : items.length - 1;
        items[i < 0 ? 0 : (i + step) % items.length]?.focus();
      }}
    >
      <div className="vmenu-title" title={v.title}>
        {v.title}
      </div>
      <button role="menuitem" onClick={blockVideo} disabled={!videoOk} title={videoOk ? undefined : "Coming soon"}>
        <EyeOff size={16} /> Hide this video
      </button>
      <button role="menuitem" className="danger" onClick={blockChannel}>
        <Ban size={16} /> Block {v.channelTitle}
      </button>
      <div className="vmenu-sep" />
      <button role="menuitem" onClick={toggleFavorite}>
        {favorite ? <StarOff size={16} /> : <Star size={16} />}
        {favorite ? "Remove channel from favorites" : "Add channel to favorites"}
      </button>
      <button role="menuitem" onClick={copy}>
        <Link2 size={16} /> Copy link
      </button>
      <button role="menuitem" onClick={mpv}>
        <MonitorPlay size={16} /> Open in mpv
      </button>
    </div>
  );
}
