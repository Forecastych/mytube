// Theme and glass live on :root, so every component reads them from CSS.
// Applied before the first render (no flash of the wrong theme) and on change.

import { onPrefChange, pref, setPref } from "./prefs";

const KEYS = new Set(["theme", "glassAlpha", "glassBlur", "reduceTransparency"]);

/** Before Settings the header toggle kept the theme under its own key. */
function migrateTheme() {
  try {
    const old = localStorage.getItem("mytube.theme");
    if (old == null) return;
    localStorage.removeItem("mytube.theme");
    if (localStorage.getItem("pref.theme") == null && old === "light") setPref("theme", "light");
  } catch {
    /* storage blocked */
  }
}

function apply() {
  const root = document.documentElement;
  root.dataset.theme = pref("theme");
  const reduce = pref("reduceTransparency");
  root.dataset.reduceTransparency = String(reduce);
  // Inline values would beat the [data-reduce-transparency] rule in tokens.css.
  if (reduce) {
    root.style.removeProperty("--glass-alpha");
    root.style.removeProperty("--glass-blur");
  } else {
    root.style.setProperty("--glass-alpha", String(pref("glassAlpha")));
    root.style.setProperty("--glass-blur", `${pref("glassBlur")}px`);
  }
}

export function initAppearance() {
  migrateTheme();
  apply();
  onPrefChange((k) => KEYS.has(k) && apply());
}
