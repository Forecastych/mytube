import { NavLink } from "react-router-dom";
import { Flame, Heart, House, Search, Settings, SlidersHorizontal, type LucideIcon } from "lucide-react";
import "./Sidebar.css";

type Item = { to: string; label: string; icon: LucideIcon };

const main: Item[] = [
  { to: "/", label: "Home", icon: House },
  { to: "/favorites", label: "Favorites", icon: Heart },
  { to: "/trending", label: "Trending", icon: Flame },
  { to: "/search", label: "Search", icon: Search },
];

const tools: Item[] = [
  { to: "/algorithm", label: "Algorithm", icon: SlidersHorizontal },
  { to: "/settings", label: "Settings", icon: Settings },
];

function NavList({ items }: { items: Item[] }) {
  return (
    <ul className="nav">
      {items.map(({ to, label, icon: Icon }) => (
        <li key={to}>
          <NavLink to={to} end={to === "/"} className="nav-item">
            <Icon size={20} strokeWidth={1.8} />
            <span>{label}</span>
          </NavLink>
        </li>
      ))}
    </ul>
  );
}

export function Sidebar() {
  return (
    <aside className="sidebar glass">
      <div className="logo">
        My<span>Tube</span>
      </div>
      <NavList items={main} />
      <div className="sidebar-spacer" />
      <NavList items={tools} />
    </aside>
  );
}
