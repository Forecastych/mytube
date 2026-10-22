import { Link } from "react-router-dom";
import type { Channel } from "../lib/api";
import "./FavoritesRow.css";

export function FavoritesRow({ channels }: { channels: Channel[] }) {
  return (
    <div className="fav-row">
      {channels.map((c) => (
        <Link to={`/channel/${c.id}`} className="fav" key={c.id} title={c.title}>
          <div className="fav-avatar">
            {c.avatar ? <img src={c.avatar} alt="" decoding="async" /> : <span>{c.title.slice(0, 1)}</span>}
            {c.newCount > 0 && <span className="fav-badge">{c.newCount}</span>}
          </div>
          <div className="fav-name">{c.title}</div>
        </Link>
      ))}
    </div>
  );
}
