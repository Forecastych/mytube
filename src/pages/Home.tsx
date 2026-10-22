import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Flame, Inbox, LayoutGrid, RotateCw, Sparkles } from "lucide-react";
import { AddChannel } from "../components/AddChannel";
import { FavoritesRow } from "../components/FavoritesRow";
import { VideoCard } from "../components/VideoCard";
import { errorText } from "../lib/api";
import { refresh, useFavorites } from "../lib/favorites";
import { cachedHome, homeFeed, type HomeFeed, type Source } from "../lib/feed";
import "./Home.css";

const NEW_LIMIT = 12;

const sources: { id: Source | null; label: string; icon: typeof Inbox }[] = [
  { id: null, label: "All", icon: LayoutGrid },
  { id: "subscriptions", label: "Subscriptions", icon: Inbox },
  { id: "trending", label: "Trending", icon: Flame },
  { id: "related", label: "Related", icon: Sparkles },
];

export default function Home() {
  const { channels, videos, failed, loading, loaded, error } = useFavorites();
  const [source, setSource] = useState<Source | null>(null);
  const fresh = useMemo(() => videos.filter((v) => v.isNew).slice(0, NEW_LIMIT), [videos]);

  return (
    <div className="home">
      <section>
        <div className="section-head">
          <h2>Favorites</h2>
          <span className="muted">Your favorite channels</span>
          <button
            className="btn btn-ghost section-action"
            onClick={() => refresh(true)}
            disabled={loading}
            title="Refresh"
          >
            <RotateCw size={16} className={loading ? "spin" : undefined} />
          </button>
        </div>
        {loaded && channels.length === 0 ? (
          <div className="home-start">
            <p className="muted">
              New uploads from favorite channels always sit at the top of Home. Paste a channel link:
            </p>
            <AddChannel />
          </div>
        ) : (
          <FavoritesRow channels={channels} />
        )}
        {error && <div className="form-error">{error}</div>}
        {failed.length > 0 && (
          <div className="muted small">
            Couldn't refresh {failed.length} channel{failed.length > 1 ? "s" : ""}, showing cached videos.
          </div>
        )}
      </section>

      {channels.length > 0 && (
        <section>
          <div className="section-head">
            <span className="dot" />
            <h2 className="accent">NEW</h2>
            <span className="muted">Latest videos from your favorite channels</span>
            <Link to="/favorites" className="muted small section-action">
              All from favorites →
            </Link>
          </div>
          {fresh.length > 0 ? (
            <div className="strip">
              {fresh.map((v) => (
                <VideoCard key={v.id} v={v} badge />
              ))}
            </div>
          ) : (
            <p className="muted">
              {loading ? "Checking for new videos…" : "Nothing new this week. You're all caught up."}
            </p>
          )}
        </section>
      )}

      <section>
        <div className="chips">
          {sources.map(({ id, label, icon: Icon }) => (
            <button key={label} className={`chip glass${id === source ? " active" : ""}`} onClick={() => setSource(id)}>
              <Icon size={16} />
              {label}
            </button>
          ))}
        </div>
        <Mixed source={source} />
      </section>
    </div>
  );
}

/** The grid under the chips: the Feed mix, or one source alone. */
function Mixed({ source }: { source: Source | null }) {
  const [feed, setFeed] = useState<HomeFeed | undefined>(() => cachedHome(source));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shown = useRef(source);
  shown.current = source;

  useEffect(() => {
    setFeed(cachedHome(source));
    setError(null);
    setLoading(true);
    homeFeed(source)
      .then((f) => shown.current === source && setFeed(f))
      .catch((e) => shown.current === source && setError(errorText(e)))
      .finally(() => shown.current === source && setLoading(false));
  }, [source]);

  if (error) return <div className="form-error">{error}</div>;
  if (!feed) return <p className="muted">{loading ? "Building your feed…" : ""}</p>;

  const signIn = (what: string) => (
    <p className="muted">
      Sign in with Google in <Link to="/settings">Settings</Link> to {what}.
    </p>
  );
  return (
    <>
      {feed.notice && <p className="muted small">{feed.notice}</p>}
      {source === "subscriptions" && !feed.signedIn && feed.subscriptions === 0 && signIn("import your subscriptions")}
      {source === "trending" && !feed.signedIn && signIn("see what's trending")}
      {source === "related" && (
        <p className="muted small">
          Videos like the latest ones from your favorites, or from subscriptions if you have no favorites yet.
        </p>
      )}
      {feed.hidden > 0 && (
        <p className="muted small">
          {feed.hidden} hidden by your <Link to="/algorithm">filters</Link>
        </p>
      )}
      {feed.videos.length > 0 ? (
        <div className="grid">
          {feed.videos.map((v) => (
            <VideoCard key={v.id} v={v} />
          ))}
        </div>
      ) : (
        !(source === "trending" && !feed.signedIn) && (
          <p className="muted">{loading ? "Loading…" : "Nothing here yet."}</p>
        )
      )}
    </>
  );
}
