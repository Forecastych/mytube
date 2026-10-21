import type { Channel } from "../lib/api";
import "./FavoritesRow.css";

export function FavoritesRow({ channels }: { channels: Channel[] }) {
  return (
    <div className="fav-row">
      {channels.map((c) => (
        <div className="fav" key={c.id} title={c.title}>
          <div className="fav-avatar">
            {c.avatar ? <img src={c.avatar} alt="" decoding="async" /> : <span>{c.title.slice(0, 1)}</span>}
            {c.newCount > 0 && <span className="fav-badge">{c.newCount}</span>}
          </div>
          <div className="fav-name">{c.title}</div>
        </div>
      ))}
    </div>
  );
}
