import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { Moon, Search, Sun } from "lucide-react";
import { useTheme } from "../lib/theme";
import "./Header.css";

export function Header() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const { theme, toggle } = useTheme();

  // search.list costs 100 units: search on submit only, never per keystroke.
  function submit(e: FormEvent) {
    e.preventDefault();
    const q = query.trim();
    if (q) navigate(`/search?q=${encodeURIComponent(q)}`);
  }

  return (
    <header className="header">
      <form className="search glass" onSubmit={submit}>
        <Search size={18} strokeWidth={1.8} />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search videos, channels…"
          spellCheck={false}
        />
      </form>
      <button className="icon-btn glass" onClick={toggle} title="Toggle theme">
        {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
      </button>
    </header>
  );
}
