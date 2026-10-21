import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Flame, Inbox, RotateCw, Star } from "lucide-react";
import { AddChannel } from "../components/AddChannel";
import { FavoritesRow } from "../components/FavoritesRow";
import { VideoCard } from "../components/VideoCard";
import { refresh, useFavorites } from "../lib/favorites";
import "./Home.css";

const NEW_LIMIT = 12;

const sources = [
  { id: "subscriptions", label: "Subscriptions", icon: Inbox, empty: "Sign in with YouTube in Settings to import your subscriptions." },
  { id: "trending", label: "Trending", icon: Flame, empty: "Trending lands with sign-in." },
  { id: "related", label: "Related to favorites", icon: Star, empty: "Related videos are coming soon." },
] as const;

export default function Home() {
  const { channels, videos, failed, loading, loaded, error } = useFavorites();
  const [source, setSource] = useState<(typeof sources)[number]["id"]>("subscriptions");
  const fresh = useMemo(() => videos.filter((v) => v.isNew).slice(0, NEW_LIMIT), [videos]);
  const active = sources.find((s) => s.id === source)!;

  if (loaded && channels.length === 0) {
    return (
      <section className="home-empty">
        <h1>Start with your favorite channels</h1>
        <p>New uploads from favorites always sit at the top of Home. Paste a channel link:</p>
        <AddChannel autoFocus />
      </section>
    );
  }

  return (
    <div className="home">
      <section>
        <div className="section-head">
          <h2>Favorites</h2>
          <span className="muted">Your favorite channels</span>
          <button className="btn btn-ghost section-action" onClick={() => refresh(true)} disabled={loading} title="Refresh">
            <RotateCw size={16} className={loading ? "spin" : undefined} />
          </button>
        </div>
        <FavoritesRow channels={channels} />
        {error && <div className="form-error">{error}</div>}
        {failed.length > 0 && (
          <div className="muted small">
            Couldn't refresh {failed.length} channel{failed.length > 1 ? "s" : ""}, showing cached videos.
          </div>
        )}
      </section>

      <section>
        <div className="section-head">
          <span className="dot" />
          <h2 className="accent">NEW</h2>
          <span className="muted">Latest videos from your favorite channels</span>
          <Link to="/favorites" className="muted small section-action">All from favorites →</Link>
        </div>
        {fresh.length > 0 ? (
          <div className="strip">
            {fresh.map((v) => (
              <VideoCard key={v.id} v={v} badge />
            ))}
          </div>
        ) : (
          <p className="muted">{loading ? "Checking for new videos…" : "Nothing new this week. You're all caught up."}</p>
        )}
      </section>

      <section>
        <div className="chips">
          {sources.map(({ id, label, icon: Icon }) => (
            <button key={id} className={`chip glass${id === source ? " active" : ""}`} onClick={() => setSource(id)}>
              <Icon size={16} />
              {label}
            </button>
          ))}
        </div>
        <p className="muted">{active.empty}</p>
      </section>
    </div>
  );
}
