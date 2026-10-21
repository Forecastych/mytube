import { HashRouter, Route, Routes } from "react-router-dom";
import { GlassFilter } from "./components/GlassFilter";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { Placeholder } from "./pages/Placeholder";
import "./App.css";

export default function App() {
  return (
    <HashRouter>
      <GlassFilter />
      <div className="shell">
        <Sidebar />
        <main className="content">
          <Header />
          <div className="page">
            <Routes>
              <Route path="/" element={<Placeholder title="Home" note="Favorites, NEW, feed by your rules." />} />
              <Route path="/favorites" element={<Placeholder title="Favorites" note="Your channels, in your order." />} />
              <Route path="/trending" element={<Placeholder title="Trending" note="By region and category." />} />
              <Route path="/search" element={<Placeholder title="Search" note="Press Enter in the search bar." />} />
              <Route path="/algorithm" element={<Placeholder title="Algorithm" note="Blocked topics, words, channels; feed mix." />} />
              <Route path="/settings" element={<Placeholder title="Settings" note="Account, appearance, network." />} />
              <Route path="/watch/:id" element={<Placeholder title="Watch" note="Player, description, comments." />} />
            </Routes>
          </div>
        </main>
      </div>
    </HashRouter>
  );
}
