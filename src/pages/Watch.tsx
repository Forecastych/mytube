import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { ArrowLeft, ChevronDown, ChevronUp, ListVideo, RotateCw, Star } from "lucide-react";
import { api, errorText, thumb, type Video, type WatchInfo } from "../lib/api";
import { addFavorite, removeFavorite, useFavorites } from "../lib/favorites";
import { ago, clock, views } from "../lib/format";
import { pref, setPref } from "../lib/prefs";
import { resumeAt } from "../lib/progress";
import { Player, PlayerPlaceholder } from "../components/player/Player";
import { WatchedBar } from "../components/VideoCard";
import { Linkified, VIDEO_ID } from "./watch/Linkified";
import { Comments } from "./watch/Comments";
import "./Watch.css";

const UP_NEXT = 20;

/** `key={id}`: a new video is a fresh page, no state leaks between videos. */
export default function WatchRoute() {
  const { id = "" } = useParams();
  return <Watch key={id} id={id} />;
}

function Watch({ id }: { id: string }) {
  const [params] = useSearchParams();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [load, setLoad] = useState({ force: false, start: Number(params.get("t")) || 0 });
  const [info, setInfo] = useState<WatchInfo | null>(null);
  // Saved position, used only when nothing else says where to start (?t=, a reload after 403).
  const [resume, setResume] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const lastExpiry = useRef(0);
  const [upNextOpen, setUpNextOpen] = useState(() => pref("upNextOpen"));

  useEffect(() => {
    if (!VIDEO_ID.test(id)) {
      setError("This doesn't look like a YouTube video link.");
      return;
    }
    let alive = true;
    setError(null);
    Promise.all([api.watchInfo(id, load.force), load.start ? 0 : resumeAt(id)]).then(
      ([i, r]) => {
        if (!alive) return;
        setResume(i.isLive ? 0 : r);
        setInfo(i);
      },
      (e) => alive && setError(errorText(e)),
    );
    return () => {
      alive = false;
    };
  }, [id, load]);

  useEffect(() => {
    if (!info) return;
    const prev = document.title;
    document.title = `${info.title} — MyTube`;
    return () => void (document.title = prev);
  }, [info]);

  // Signed stream URLs live ~6 h; on 403 fetch fresh ones and resume. Twice in a
  // row means something else is wrong, so stop and show it instead of looping.
  const onExpired = useCallback((t: number, manual = false) => {
    const now = Date.now();
    if (!manual && now - lastExpiry.current < 30_000) {
      setInfo(null);
      setError("YouTube keeps refusing the stream. Try again later or open it in mpv.");
      return;
    }
    lastExpiry.current = now;
    setInfo(null);
    setLoad({ force: true, start: t });
  }, []);

  const seek = useCallback((t: number) => {
    const v = videoRef.current;
    if (!v) return;
    v.currentTime = t;
    void v.play().catch(() => {});
    v.closest(".player")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, []);

  const toggleUpNext = () => {
    setUpNextOpen(!upNextOpen);
    setPref("upNextOpen", !upNextOpen);
  };

  return (
    <div className={`watch${upNextOpen ? "" : " wide"}`}>
      <div className="watch-main">
        <div className="watch-top">
          <BackButton />
          {!upNextOpen && (
            <button className="chip glass" onClick={toggleUpNext}>
              <ListVideo size={16} /> Up next
            </button>
          )}
        </div>

        {info ? (
          <Player info={info} startAt={load.start || resume} resumed={!load.start && resume > 0} onExpired={onExpired} videoRef={videoRef} />
        ) : (
          <PlayerPlaceholder id={id}>
            {error && (
              <>
                <p>{error}</p>
                {VIDEO_ID.test(id) && (
                  <button className="btn" onClick={() => setLoad((l) => ({ ...l, force: true }))}>
                    <RotateCw size={16} /> Retry
                  </button>
                )}
              </>
            )}
          </PlayerPlaceholder>
        )}

        {info && <Details info={info} onSeek={seek} />}
        {info && !info.isLive && <Comments id={info.id} onSeek={seek} />}
      </div>

      {upNextOpen && <UpNext current={id} onCollapse={toggleUpNext} />}
    </div>
  );
}

function BackButton() {
  const navigate = useNavigate();
  const location = useLocation();
  // Opened directly (no history in this app): go Home instead of leaving nowhere.
  const back = () => (location.key === "default" ? navigate("/") : navigate(-1));
  return (
    <button className="watch-back" onClick={back}>
      <ArrowLeft size={18} /> Back
    </button>
  );
}

const Details = memo(function Details({ info, onSeek }: { info: WatchInfo; onSeek: (t: number) => void }) {
  const { channels } = useFavorites();
  const [fav, setFav] = useState(info.isFavorite);
  const [busy, setBusy] = useState(false);
  const [favError, setFavError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const avatar = channels.find((c) => c.id === info.channelId)?.avatar;

  const toggleFavorite = async () => {
    if (!info.channelId) return;
    setBusy(true);
    setFavError(null);
    try {
      if (fav) await removeFavorite(info.channelId);
      else await addFavorite(info.channelId);
      setFav(!fav);
    } catch (e) {
      setFavError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const long = info.description.length > 180 || info.description.split("\n").length > 2;

  return (
    <section className="watch-details">
      <h1 className="watch-title">{info.title}</h1>
      <div className="watch-meta muted">
        {[views(info.views), info.published ? ago(info.published) : "", info.isLive ? "LIVE" : ""]
          .filter(Boolean)
          .join(" · ")}
      </div>

      <div className="watch-channel">
        {avatar ? (
          <img className="watch-avatar" src={avatar} alt="" decoding="async" />
        ) : (
          <span className="watch-avatar">{info.channel.slice(0, 1)}</span>
        )}
        <div className="watch-channel-name">{info.channel}</div>
        <span className="spacer" />
        {info.channelId && (
          <button
            className={`btn ${fav ? "" : "btn-fav"}`}
            onClick={toggleFavorite}
            disabled={busy}
            title={fav ? "Remove from favorites" : undefined}
          >
            <Star size={16} fill={fav ? "currentColor" : "none"} />
            {fav ? "In favorites" : "Add to favorites"}
          </button>
        )}
      </div>
      {favError && <p className="watch-error small">{favError}</p>}

      {info.description && (
        <div className="watch-desc">
          <div className={`watch-desc-text${expanded ? "" : " clamped"}`}>
            <Linkified text={info.description} onSeek={onSeek} />
          </div>
          {long && (
            <button className="watch-more" onClick={() => setExpanded(!expanded)}>
              {expanded ? "Show less" : "Show more"}
              {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
            </button>
          )}
        </div>
      )}

      {info.chapters.length > 1 && (
        <div className="watch-chapters">
          {info.chapters.map((c) => (
            <button key={c.startTime} className="chip" onClick={() => onSeek(c.startTime)}>
              <span className="accent">{clock(c.startTime)}</span> {c.title}
            </button>
          ))}
        </div>
      )}

    </section>
  );
});

function UpNext({ current, onCollapse }: { current: string; onCollapse: () => void }) {
  const { videos, loaded } = useFavorites();
  const list = useMemo(() => videos.filter((v) => v.id !== current).slice(0, UP_NEXT), [videos, current]);
  return (
    <aside className="upnext">
      <div className="upnext-head">
        <h2>Up next</h2>
        <button className="btn btn-ghost" onClick={onCollapse} title="Collapse Up next">
          <ChevronUp size={18} />
        </button>
      </div>
      <div className="upnext-note muted small">From your favorites</div>
      {list.map((v) => (
        <UpNextItem key={v.id} v={v} />
      ))}
      {loaded && list.length === 0 && (
        <p className="muted small">Add channels to favorites to see their new videos here.</p>
      )}
      <button className="upnext-collapse" onClick={onCollapse}>
        <ChevronDown size={16} /> Collapse Up next
      </button>
    </aside>
  );
}

const UpNextItem = memo(function UpNextItem({ v }: { v: Video }) {
  return (
    <Link to={`/watch/${v.id}`} className="upnext-item" title={v.title}>
      <div className="upnext-thumb">
        <img src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
        {v.isNew && <span className="badge-new">NEW</span>}
        <WatchedBar id={v.id} />
      </div>
      <div className="upnext-text">
        <div className="upnext-title">{v.title}</div>
        <div className="muted small">{v.channelTitle}</div>
        <div className="muted small">
          {views(v.views)}
          {v.views != null && " · "}
          {ago(v.published)}
        </div>
      </div>
    </Link>
  );
});
