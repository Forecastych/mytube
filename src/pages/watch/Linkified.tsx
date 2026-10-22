import { useMemo, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { openUrl } from "@tauri-apps/plugin-opener";

export const VIDEO_ID = /^[\w-]{11}$/;

// URLs and 1:23 / 1:02:03 timestamps become clickable; everything else stays text.
const TOKEN = /(https?:\/\/[^\s<>"]+[^\s<>".,;:!?)\]])|(?<![\w:])((?:\d{1,2}:)?\d{1,2}:\d{2})(?![\w:])/g;

function seconds(ts: string) {
  return ts.split(":").reduce((acc, p) => acc * 60 + Number(p), 0);
}

/** A youtube.com/youtu.be video link opens here instead of the browser. */
function internalId(url: string): string | null {
  try {
    const u = new URL(url);
    const host = u.hostname.replace(/^(www\.|m\.)/, "");
    const id =
      host === "youtu.be"
        ? u.pathname.slice(1)
        : host === "youtube.com" && u.pathname === "/watch"
          ? u.searchParams.get("v")
          : host === "youtube.com" && u.pathname.startsWith("/shorts/")
            ? u.pathname.slice(8)
            : null;
    return id && VIDEO_ID.test(id) ? id : null;
  } catch {
    return null;
  }
}

export function Linkified({ text, onSeek }: { text: string; onSeek: (t: number) => void }) {
  const parts = useMemo(() => {
    const out: ReactNode[] = [];
    let last = 0;
    for (const m of text.matchAll(TOKEN)) {
      const i = m.index;
      if (i > last) out.push(text.slice(last, i));
      const [whole, url, ts] = m;
      if (url) {
        const vid = internalId(url);
        out.push(
          vid ? (
            <Link key={i} to={`/watch/${vid}`}>
              {url}
            </Link>
          ) : (
            <a
              key={i}
              href={url}
              onClick={(e) => {
                e.preventDefault();
                void openUrl(url).catch(() => {});
              }}
            >
              {url}
            </a>
          ),
        );
      } else {
        out.push(
          <button key={i} className="watch-ts" onClick={() => onSeek(seconds(ts))}>
            {ts}
          </button>,
        );
      }
      last = i + whole.length;
    }
    if (last < text.length) out.push(text.slice(last));
    return out;
  }, [text, onSeek]);
  return <>{parts}</>;
}
