import { lazy, Suspense } from "react";
import { HashRouter, Route, Routes } from "react-router-dom";
import { ScreenBoundary } from "./components/ErrorBoundary";
import { GlassFilter } from "./components/GlassFilter";
import { Header } from "./components/Header";
import { Sidebar } from "./components/Sidebar";
import { VideoMenu } from "./components/VideoMenu";
import "./App.css";

// Screens load on first visit: keeps the startup bundle small.
const Home = lazy(() => import("./pages/Home"));
const Favorites = lazy(() => import("./pages/Favorites"));
const Watch = lazy(() => import("./pages/Watch"));
const Algorithm = lazy(() => import("./pages/Algorithm"));
const Settings = lazy(() => import("./pages/Settings"));
const Search = lazy(() => import("./pages/Search"));
const Trending = lazy(() => import("./pages/Trending"));
const Channel = lazy(() => import("./pages/Channel"));

export default function App() {
  return (
    <HashRouter>
      <GlassFilter />
      <div className="shell">
        <Sidebar />
        <main className="content">
          <Header />
          <div className="page">
            <ScreenBoundary>
              <Suspense fallback={null}>
                <Routes>
                  <Route path="/" element={<Home />} />
                  <Route path="/favorites" element={<Favorites />} />
                  <Route path="/trending" element={<Trending />} />
                  <Route path="/search" element={<Search />} />
                  <Route path="/algorithm" element={<Algorithm />} />
                  <Route path="/settings" element={<Settings />} />
                  <Route path="/watch/:id" element={<Watch />} />
                  <Route path="/channel/:id" element={<Channel />} />
                </Routes>
              </Suspense>
            </ScreenBoundary>
          </div>
        </main>
      </div>
      <VideoMenu />
    </HashRouter>
  );
}
