import { memo } from "react";
import { Link } from "react-router-dom";
import { thumb, type Video } from "../lib/api";
import { ago, views } from "../lib/format";
import { useHidden } from "../lib/hidden";
import { openVideoMenu } from "./VideoMenu";
import "./VideoCard.css";

export const VideoCard = memo(function VideoCard({ v, badge }: { v: Video; badge?: boolean }) {
  if (useHidden(v.id, v.channelId)) return null;
  return (
    <Link to={`/watch/${v.id}`} className="vcard" title={v.title} onContextMenu={(e) => openVideoMenu(e, v)}>
      <div className="vcard-thumb">
        <img src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
        {badge && v.isNew && <span className="badge-new">NEW</span>}
      </div>
      <div className="vcard-body">
        {v.channelAvatar ? (
          <img className="vcard-avatar" src={v.channelAvatar} alt="" loading="lazy" decoding="async" />
        ) : (
          <span className="vcard-avatar" />
        )}
        <div className="vcard-text">
          <div className="vcard-title">{v.title}</div>
          <div className="vcard-meta">{v.channelTitle}</div>
          <div className="vcard-meta">
            {views(v.views)}
            {v.views != null && " · "}
            {ago(v.published)}
          </div>
        </div>
      </div>
    </Link>
  );
});
