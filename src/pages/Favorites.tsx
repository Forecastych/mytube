import { Link } from "react-router-dom";
import { X } from "lucide-react";
import { AddChannel } from "../components/AddChannel";
import { VideoCard } from "../components/VideoCard";
import { removeFavorite, useFavorites } from "../lib/favorites";
import "./Favorites.css";

export default function Favorites() {
  const { channels, videos, loaded } = useFavorites();

  return (
    <div className="favorites">
      <div className="section-head">
        <h1>Favorites</h1>
        <span className="muted">{channels.length} channels</span>
      </div>

      <AddChannel />

      {channels.length > 0 && (
        <ul className="fav-list">
          {channels.map((c) => (
            <li key={c.id} className="fav-pill">
              <Link to={`/channel/${c.id}`} className="fav-pill-link" title={`Open ${c.title}`}>
                {c.avatar && <img src={c.avatar} alt="" decoding="async" />}
                <span>{c.title}</span>
              </Link>
              <button
                className="fav-remove"
                title={`Remove ${c.title}`}
                onClick={() => removeFavorite(c.id)}
              >
                <X size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {loaded && videos.length > 0 && (
        <div className="grid">
          {videos.map((v) => (
            <VideoCard key={v.id} v={v} badge />
          ))}
        </div>
      )}
    </div>
  );
}
