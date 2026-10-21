import { lazy, Suspense } from "react";
import { HashRouter, Route, Routes } from "react-router-dom";
import { GlassFilter } from "./components/GlassFilter";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { Placeholder } from "./pages/Placeholder";
import "./App.css";

// Screens load on first visit: keeps the startup bundle small.
const Home = lazy(() => import("./pages/Home"));
const Favorites = lazy(() => import("./pages/Favorites"));

export default function App() {
  return (
    <HashRouter>
      <GlassFilter />
      <div className="shell">
        <Sidebar />
        <main className="content">
          <Header />
          <div className="page">
            <Suspense fallback={null}>
              <Routes>
                <Route path="/" element={<Home />} />
                <Route path="/favorites" element={<Favorites />} />
                <Route path="/trending" element={<Placeholder title="Trending" note="By region and category." />} />
                <Route path="/search" element={<Placeholder title="Search" note="Press Enter in the search bar." />} />
                <Route path="/algorithm" element={<Placeholder title="Algorithm" note="Blocked topics, words, channels; feed mix." />} />
                <Route path="/settings" element={<Placeholder title="Settings" note="Account, appearance, network." />} />
                <Route path="/watch/:id" element={<Placeholder title="Watch" note="Player, description, comments." />} />
              </Routes>
            </Suspense>
          </div>
        </main>
      </div>
    </HashRouter>
  );
}
