import { memo } from "react";
import { Link } from "react-router-dom";
import { thumb, type Video } from "../lib/api";
import { ago, views } from "../lib/format";
import { useHidden } from "../lib/hidden";
import { useProgress } from "../lib/progress";
import { openVideoMenu } from "./VideoMenu";
import "./VideoCard.css";

export const VideoCard = memo(function VideoCard({ v, badge }: { v: Video; badge?: boolean }) {
  const hidden = useHidden(v.id, v.channelId);
  if (hidden) return null;
  const channel = `/channel/${v.channelId}`;
  // The title link stretches over the whole card; the channel links sit above it.
  return (
    <div className="vcard" title={v.title} onContextMenu={(e) => openVideoMenu(e, v)}>
      <div className="vcard-thumb">
        <img src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
        {badge && v.isNew && <span className="badge-new">NEW</span>}
        <WatchedBar id={v.id} />
      </div>
      <div className="vcard-body">
        <Link to={channel} className="vcard-avatar" title={v.channelTitle} tabIndex={-1}>
          {v.channelAvatar && <img src={v.channelAvatar} alt="" loading="lazy" decoding="async" />}
        </Link>
        <div className="vcard-text">
          <Link to={`/watch/${v.id}`} className="vcard-title">
            {v.title}
          </Link>
          <div className="vcard-meta">
            <Link to={channel} className="vcard-channel" title={v.channelTitle}>
              {v.channelTitle}
            </Link>
          </div>
          <div className="vcard-meta">
            {views(v.views)}
            {v.views != null && " · "}
            {ago(v.published)}
          </div>
        </div>
      </div>
    </div>
  );
});

/** How much of the video was watched; nothing for unstarted and finished ones. */
export function WatchedBar({ id }: { id: string }) {
  const p = useProgress(id);
  if (p == null) return null;
  return (
    <span className="watched-bar">
      <span style={{ transform: `scaleX(${Math.min(1, p)})` }} />
    </span>
  );
}
