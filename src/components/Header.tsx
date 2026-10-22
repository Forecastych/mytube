import { useEffect, useState, type FormEvent } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { Moon, Search, Sun } from "lucide-react";
import { usePref } from "../lib/prefs";
import "./Header.css";

export function Header() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [theme, setTheme] = usePref("theme");
  const { pathname } = useLocation();
  const [params] = useSearchParams();
  const shown = pathname === "/search" ? (params.get("q") ?? "") : null;

  // The box shows the query of the results on screen (back/forward included).
  useEffect(() => {
    if (shown != null) setQuery(shown);
  }, [shown]);

  // On submit only, never per keystroke: each search is a request to YouTube.
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
      <button className="icon-btn glass" onClick={() => setTheme(theme === "dark" ? "light" : "dark")} title="Toggle theme">
        {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
      </button>
    </header>
  );
}
