import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Check, LoaderCircle, RotateCw, Star } from "lucide-react";
import { VideoCard } from "../components/VideoCard";
import { api, errorText, type SearchChannel, type Video } from "../lib/api";
import { addFavorite, useFavorites } from "../lib/favorites";
import "./Search.css";

type Results = {
  channels: SearchChannel[];
  videos: Video[];
  hidden: number;
  next: string | null;
  loaded: boolean;
};

const empty: Results = { channels: [], videos: [], hidden: 0, next: null, loaded: false };

// Recent results and where they were scrolled to: back from a video (or from
// another query) lands on the same spot instead of a fresh first page.
type Kept = { results: Results; scroll: number; at: number };
const kept = new Map<string, Kept>();
const KEEP = 5;
/** Same as the Rust-side cache: a query repeated later gets fresh results. */
const KEEP_MS = 10 * 60 * 1000;

const scroller = () => document.querySelector<HTMLElement>(".content");

export default function Search() {
  const [params] = useSearchParams();
  const q = (params.get("q") ?? "").trim();
  if (!q) {
    return (
      <section className="placeholder">
        <h1>Search</h1>
        <p>Type in the search bar and press Enter.</p>
      </section>
    );
  }
  // A fresh instance per query: no state from the previous one leaks in.
  return <SearchResults key={q} q={q} />;
}

function SearchResults({ q }: { q: string }) {
  const [restored] = useState(() => {
    const k = kept.get(q);
    return k && Date.now() - k.at < KEEP_MS ? k : null;
  });
  const [results, setResults] = useState<Results>(restored?.results ?? empty);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(results);
  latest.current = results;
  // Bumped on cleanup: an answer for an unmounted (or StrictMode-remounted) page is dropped.
  const gen = useRef(0);
  const scroll = useRef(0);

  const load = useCallback(
    async (next: string | null) => {
      const my = gen.current;
      setBusy(true);
      setError(null);
      try {
        const page = await api.search(q, next);
        if (my !== gen.current) return;
        setResults((r) => {
          const seen = new Set(r.videos.map((v) => v.id));
          return {
            channels: next ? r.channels : page.channels,
            videos: [...r.videos, ...page.videos.filter((v) => !seen.has(v.id))],
            hidden: r.hidden + page.hidden,
            next: page.next,
            loaded: true,
          };
        });
      } catch (e) {
        if (my === gen.current) setError(errorText(e));
      } finally {
        if (my === gen.current) setBusy(false);
      }
    },
    [q],
  );

  // Layout effect: restores the scroll before paint, and its cleanup runs
  // while this page still owns the scroller.
  useLayoutEffect(() => {
    const el = scroller();
    if (restored) {
      if (el) el.scrollTop = scroll.current = restored.scroll;
    } else {
      el?.scrollTo({ top: 0 });
      void load(null);
    }
    const onScroll = () => (scroll.current = el!.scrollTop);
    el?.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      gen.current++;
      el?.removeEventListener("scroll", onScroll);
      if (!latest.current.loaded) return;
      kept.delete(q); // re-insert: Map order is the eviction order
      kept.set(q, { results: latest.current, scroll: scroll.current, at: restored?.at ?? Date.now() });
      if (kept.size > KEEP) kept.delete(kept.keys().next().value!);
    };
  }, [q, restored, load]);

  // Next page a screen before the end.
  const sentinel = useRef<HTMLDivElement>(null);
  const more = results.next && !busy && !error ? () => load(results.next) : null;
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !more) return;
    const io = new IntersectionObserver(([e]) => e.isIntersecting && more(), { rootMargin: "800px 0px" });
    io.observe(el);
    return () => io.disconnect();
  }, [more]);

  const nothing = results.loaded && !results.videos.length && !results.channels.length;
  return (
    <div className="search-page">
      <div className="section-head">
        <h2>Results for “{q}”</h2>
        {results.hidden > 0 && <span className="muted small">{results.hidden} hidden by your filters</span>}
      </div>

      {results.channels.length > 0 && (
        <div className="search-channels">
          {results.channels.map((c) => (
            <ChannelHit key={c.id} c={c} />
          ))}
        </div>
      )}

      {results.videos.length > 0 && (
        <div className="grid">
          {results.videos.map((v) => (
            <VideoCard key={v.id} v={v} />
          ))}
        </div>
      )}

      {nothing && <p className="muted">Nothing found.</p>}

      <div ref={sentinel} className="search-foot">
        {busy && <LoaderCircle size={28} className="spin muted" />}
        {error && (
          <>
            <span className="muted">{error}</span>
            <button className="btn" onClick={() => load(results.loaded ? results.next : null)}>
              <RotateCw size={16} /> Retry
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function ChannelHit({ c }: { c: SearchChannel }) {
  const { channels } = useFavorites();
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isFavorite = channels.some((f) => f.id === c.id);

  async function add() {
    setAdding(true);
    setError(null);
    try {
      await addFavorite(c.id);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setAdding(false);
    }
  }

  return (
    <div className="search-channel" title={error ?? undefined}>
      {c.avatar ? <img src={c.avatar} alt="" loading="lazy" decoding="async" /> : <span className="search-avatar" />}
      <div className="search-channel-text">
        <div className="search-channel-title">{c.title}</div>
        <div className="muted small">{[c.handle, c.subscribers].filter(Boolean).join(" · ")}</div>
        {error && <div className="small search-error">{error}</div>}
      </div>
      <button className="btn" disabled={isFavorite || adding} onClick={add}>
        {isFavorite ? <Check size={16} /> : adding ? <LoaderCircle size={16} className="spin" /> : <Star size={16} />}
        {isFavorite ? "In favorites" : "Add to favorites"}
      </button>
    </div>
  );
}
