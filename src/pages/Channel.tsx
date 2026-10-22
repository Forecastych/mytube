import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Ban, ChevronDown, ChevronUp, ExternalLink, RotateCw, Star } from "lucide-react";
import { api, errorText, type ChannelPage } from "../lib/api";
import { setChannelBlocked } from "../lib/algorithm";
import { addFavorite, removeFavorite } from "../lib/favorites";
import { hide } from "../lib/hidden";
import { VideoCard } from "../components/VideoCard";
import { Linkified } from "./watch/Linkified";
import "./Channel.css";

const CHANNEL_ID = /^UC[\w-]{22}$/;
/** Back from a video to the channel shouldn't ask YouTube again. */
const TTL_MS = 10 * 60_000;
const cache = new Map<string, { at: number; page: ChannelPage }>();

export default function ChannelRoute() {
  const { id = "" } = useParams();
  if (!CHANNEL_ID.test(id)) return <p className="muted">This doesn't look like a channel link.</p>;
  return <Channel key={id} id={id} />;
}

function Channel({ id }: { id: string }) {
  const fresh = cache.get(id);
  const [page, setPage] = useState<ChannelPage | null>(fresh && Date.now() - fresh.at < TTL_MS ? fresh.page : null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const keep = (p: ChannelPage) => {
    cache.set(id, { at: Date.now(), page: p });
    setPage(p);
  };

  const load = () => {
    setLoading(true);
    setError(null);
    api
      .channelPage(id)
      .then(keep, (e) => setError(errorText(e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    if (!page) load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const act = (fn: () => Promise<Partial<ChannelPage>>) => async () => {
    setBusy(true);
    setError(null);
    try {
      const patch = await fn();
      if (page) keep({ ...page, ...patch });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!page) {
    return (
      <div className="channel">
        {error ? (
          <div className="form-error">
            {error}{" "}
            <button className="btn" onClick={load}>
              Try again
            </button>
          </div>
        ) : (
          <p className="muted">Loading the channel…</p>
        )}
      </div>
    );
  }

  const toggleFavorite = act(async () => {
    if (page.isFavorite) await removeFavorite(id);
    else await addFavorite(id);
    return { isFavorite: !page.isFavorite };
  });
  const toggleBlocked = act(async () => {
    const blocked = !page.isBlocked;
    await setChannelBlocked({ id, title: page.title, avatar: page.avatar }, blocked);
    hide("channel", id, blocked);
    return { isBlocked: blocked };
  });

  const desc = page.description ?? "";
  const long = desc.length > 200 || desc.split("\n").length > 3;
  const facts = [page.handle, page.subscribers, page.videoCount].filter(Boolean).join(" · ");

  return (
    <div className="channel">
      {page.banner && <img className="channel-banner" src={page.banner} alt="" decoding="async" />}

      <header className="channel-head">
        {page.avatar ? (
          <img className="channel-avatar" src={page.avatar} alt="" decoding="async" />
        ) : (
          <span className="channel-avatar">{page.title.slice(0, 1)}</span>
        )}
        <div className="channel-info">
          <h1>{page.title}</h1>
          {facts && <div className="muted">{facts}</div>}
          {page.isSubscribed && <div className="muted small">You're subscribed on YouTube</div>}
          <div className="channel-actions">
            <button className={`btn ${page.isFavorite ? "" : "btn-primary"}`} onClick={toggleFavorite} disabled={busy}>
              <Star size={16} fill={page.isFavorite ? "currentColor" : "none"} />
              {page.isFavorite ? "In favorites" : "Add to favorites"}
            </button>
            <button
              className="btn"
              onClick={() => void openUrl(`https://www.youtube.com/channel/${id}`).catch(() => {})}
              title="Open on youtube.com"
            >
              <ExternalLink size={16} /> YouTube
            </button>
            <button className={`btn ${page.isBlocked ? "" : "btn-danger"}`} onClick={toggleBlocked} disabled={busy}>
              <Ban size={16} /> {page.isBlocked ? "Unblock" : "Block"}
            </button>
            <button className="btn btn-ghost" onClick={load} disabled={loading} title="Refresh">
              <RotateCw size={16} className={loading ? "spin" : undefined} />
            </button>
          </div>
        </div>
      </header>

      {error && <div className="form-error">{error}</div>}

      {desc && (
        <div className="channel-desc">
          <div className={`channel-desc-text${expanded ? "" : " clamped"}`}>
            <Linkified text={desc} onSeek={() => {}} />
          </div>
          {long && (
            <button className="channel-more" onClick={() => setExpanded(!expanded)}>
              {expanded ? "Show less" : "Show more"}
              {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
            </button>
          )}
        </div>
      )}

      {page.isBlocked ? (
        <p className="muted">You blocked this channel: its videos are hidden everywhere in MyTube.</p>
      ) : (
        <>
          <div className="section-head">
            <h2>Latest videos</h2>
            {page.hidden > 0 && <span className="muted small">{page.hidden} Shorts hidden</span>}
          </div>
          {page.videos.length > 0 ? (
            <div className="grid">
              {page.videos.map((v) => (
                <VideoCard key={v.id} v={v} />
              ))}
            </div>
          ) : (
            <p className="muted">No videos YouTube shares for this channel.</p>
          )}
          <p className="muted small channel-note">
            YouTube shares only the latest uploads without an account. Older videos are on the channel's YouTube page.
          </p>
        </>
      )}
    </div>
  );
}
