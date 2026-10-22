import { memo, useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { RotateCw } from "lucide-react";
import { VideoCard } from "../components/VideoCard";
import { openVideoMenu } from "../components/VideoMenu";
import { errorText, thumb, type Video } from "../lib/api";
import { cachedChart, chartKey, guessRegion, trending, type Chart } from "../lib/feed";
import { ago, views } from "../lib/format";
import { useGoogle } from "../lib/google";
import { useHidden } from "../lib/hidden";
import "./Trending.css";

const CATEGORIES: { id: number | null; label: string }[] = [
  { id: null, label: "All" },
  { id: 10, label: "Music" },
  { id: 20, label: "Gaming" },
  { id: 24, label: "Entertainment" },
  { id: 25, label: "News" },
  { id: 17, label: "Sports" },
  { id: 28, label: "Science & Tech" },
  { id: 27, label: "Education" },
  { id: 1, label: "Film" },
];
const TOP = 20;

function regions(): { id: string; label: string }[] {
  const base = [
    { id: "RU", label: "RU" },
    { id: "US", label: "US" },
  ];
  const own = guessRegion();
  if (own !== "GLOBAL" && !base.some((r) => r.id === own)) base.unshift({ id: own, label: own });
  return [...base, { id: "GLOBAL", label: "Global" }];
}

function savedRegion() {
  try {
    return localStorage.getItem("trending.region") ?? guessRegion();
  } catch {
    return guessRegion();
  }
}

export default function Trending() {
  const { status } = useGoogle();
  const [region, setRegion] = useState(savedRegion);
  const [category, setCategory] = useState<number | null>(null);
  const [chart, setChart] = useState<Chart | undefined>(() => cachedChart(region, category));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const signedIn = !!status?.account;
  // A slow reply for a tab the user already left must not overwrite the new one.
  const shown = useRef(chartKey(region, category));
  shown.current = chartKey(region, category);

  function load(r = region, c = category) {
    setLoading(true);
    setError(null);
    trending(r, c)
      .then((ch) => {
        if (shown.current === chartKey(r, c)) setChart(ch);
      })
      .catch((e) => setError(errorText(e)))
      .finally(() => setLoading(false));
  }

  useEffect(() => {
    try {
      localStorage.setItem("trending.region", region);
    } catch {}
    setChart(cachedChart(region, category));
    if (signedIn) load(region, category);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [region, category, signedIn]);

  if (status && !signedIn) {
    return (
      <section className="trending-empty">
        <h1>Trending</h1>
        <p className="muted">
          YouTube removed its Trending page, MyTube builds it from the YouTube API. Sign in with your Google account in{" "}
          <Link to="/settings">Settings</Link> to open it.
        </p>
      </section>
    );
  }

  const top = chart?.videos.slice(0, TOP) ?? [];
  const rest = chart?.videos.slice(TOP) ?? [];
  return (
    <div className="trending">
      <div className="section-head">
        <h1>Trending</h1>
        <div className="trending-regions">
          {regions().map((r) => (
            <button key={r.id} className={`chip glass${r.id === region ? " active" : ""}`} onClick={() => setRegion(r.id)}>
              {r.label}
            </button>
          ))}
        </div>
        <button className="btn btn-ghost section-action" onClick={() => load()} disabled={loading} title="Refresh">
          <RotateCw size={16} className={loading ? "spin" : undefined} />
        </button>
      </div>
      <div className="chips">
        {CATEGORIES.map((c) => (
          <button key={c.label} className={`chip glass${c.id === category ? " active" : ""}`} onClick={() => setCategory(c.id)}>
            {c.label}
          </button>
        ))}
      </div>

      {error && <div className="form-error">{error}</div>}
      {chart && chart.hidden > 0 && (
        <p className="muted small">
          {chart.hidden} hidden by your <Link to="/algorithm">filters</Link>
        </p>
      )}
      {!chart && loading && <p className="muted">Loading the chart…</p>}
      {chart && chart.videos.length === 0 && <p className="muted">No chart for this category here.</p>}

      {top.length > 0 && (
        <div className="trending-top">
          {top.map((v, i) => (
            <Row key={v.id} v={v} n={i + 1} />
          ))}
        </div>
      )}
      {rest.length > 0 && (
        <>
          <h2 className="trending-more">More trending</h2>
          <div className="grid">
            {rest.map((v) => (
              <VideoCard key={v.id} v={v} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

const Row = memo(function Row({ v, n }: { v: Video; n: number }) {
  const hidden = useHidden(v.id, v.channelId);
  if (hidden) return null;
  return (
    <Link to={`/watch/${v.id}`} className="trow" title={v.title} onContextMenu={(e) => openVideoMenu(e, v)}>
      <span className="trow-n">{n}</span>
      <img className="trow-thumb" src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
      <span className="trow-text">
        <span className="trow-title">{v.title}</span>
        <span className="muted small">{v.channelTitle}</span>
        <span className="muted small">
          {views(v.views)}
          {v.views != null && " · "}
          {ago(v.published)}
        </span>
      </span>
    </Link>
  );
});
