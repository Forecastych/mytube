import { useEffect, useState } from "react";
import { Moon, Package, Palette, Play, RefreshCw, RotateCcw, Sun } from "lucide-react";
import { Switch } from "../components/Switch";
import { api, errorText } from "../lib/api";
import { defaults, setPref, usePref } from "../lib/prefs";
import { Account } from "./settings/Account";
import { Card, Row } from "./settings/parts";
import "./Settings.css";

const QUALITIES = [
  { h: 2160, label: "2160p (4K)" },
  { h: 1440, label: "1440p" },
  { h: 1080, label: "1080p" },
  { h: 720, label: "720p" },
  { h: 480, label: "480p" },
  { h: 360, label: "360p" },
];

const SPONSOR_CATEGORIES = [
  { id: "sponsor", label: "Sponsor", hint: "Paid promotion" },
  { id: "selfpromo", label: "Self-promotion", hint: "Merch, Patreon, the author's other channels" },
  { id: "interaction", label: "Interaction reminder", hint: "“Like and subscribe”" },
  { id: "intro", label: "Intro", hint: "Intro animation, pause before content" },
  { id: "outro", label: "Outro", hint: "End cards and credits" },
  { id: "preview", label: "Preview", hint: "Recap or teaser of what's coming" },
  { id: "filler", label: "Filler", hint: "Off-topic tangents and jokes" },
  { id: "music_offtopic", label: "Non-music", hint: "Talking in music videos" },
];

export default function Settings() {
  return (
    <div className="settings">
      <header className="settings-head">
        <h1>Settings</h1>
        <p className="muted">Stored on this computer only.</p>
      </header>
      <Account />
      <Appearance />
      <Playback />
      <YtDlp />
    </div>
  );
}

function Appearance() {
  const [theme, setTheme] = usePref("theme");
  const [alpha, setAlpha] = usePref("glassAlpha");
  const [blur, setBlur] = usePref("glassBlur");
  const [reduce, setReduce] = usePref("reduceTransparency");

  const reset = () => {
    setPref("glassAlpha", defaults.glassAlpha);
    setPref("glassBlur", defaults.glassBlur);
    setPref("reduceTransparency", defaults.reduceTransparency);
  };

  return (
    <Card icon={Palette} title="Appearance">
      <Row label="Theme">
        <div className="segmented" role="radiogroup" aria-label="Theme">
          {(["dark", "light"] as const).map((t) => (
            <button key={t} role="radio" aria-checked={theme === t} className={theme === t ? "active" : ""} onClick={() => setTheme(t)}>
              {t === "dark" ? <Moon size={15} /> : <Sun size={15} />}
              {t === "dark" ? "Dark" : "Light"}
            </button>
          ))}
        </div>
      </Row>

      <div className="glass-demo" aria-hidden>
        <div className="glass-demo-art" />
        <div className="glass glass-demo-chip">Glass preview</div>
      </div>

      <Row label="Glass opacity" value={`${Math.round(alpha * 100)}%`} disabled={reduce}>
        <input
          type="range"
          min={0.15}
          max={0.95}
          step={0.05}
          value={alpha}
          disabled={reduce}
          onChange={(e) => setAlpha(Number(e.target.value))}
        />
      </Row>
      <Row label="Blur" value={`${blur} px`} disabled={reduce}>
        <input
          type="range"
          min={0}
          max={40}
          step={1}
          value={blur}
          disabled={reduce}
          onChange={(e) => setBlur(Number(e.target.value))}
        />
      </Row>
      <Row label="Reduce transparency" hint="Solid panels without blur: easier to read, lighter on the GPU.">
        <Switch checked={reduce} onChange={setReduce} label="Reduce transparency" />
      </Row>

      <div className="settings-actions">
        <button className="btn" onClick={reset}>
          <RotateCcw size={15} /> Reset glass
        </button>
      </div>
    </Card>
  );
}

function Playback() {
  const [quality, setQuality] = usePref("quality");
  const [sponsor, setSponsor] = usePref("sponsorBlock");
  const [skip, setSkip] = usePref("skipCategories");

  const toggle = (id: string, on: boolean) => setSkip(on ? [...skip, id] : skip.filter((c) => c !== id));

  return (
    <Card icon={Play} title="Playback">
      <Row
        label="Default quality"
        hint="The closest available one is used; picking one in the player changes it too. Above 1080p YouTube often has VP9 only: that plays in mpv."
      >
        <select className="field" value={quality} onChange={(e) => setQuality(Number(e.target.value))}>
          {QUALITIES.map((q) => (
            <option key={q.h} value={q.h}>
              {q.label}
            </option>
          ))}
        </select>
      </Row>

      <Row label="SponsorBlock" hint="Community-marked segments. Lookups use a hash prefix, so the server doesn't learn what you watch.">
        <Switch checked={sponsor} onChange={setSponsor} label="SponsorBlock" />
      </Row>

      <div className={`settings-cats${sponsor ? "" : " disabled"}`}>
        <div className="muted small">Skip automatically</div>
        {SPONSOR_CATEGORIES.map((c) => (
          <label key={c.id} className="settings-cat">
            <input
              type="checkbox"
              checked={skip.includes(c.id)}
              disabled={!sponsor}
              onChange={(e) => toggle(c.id, e.target.checked)}
            />
            <span>{c.label}</span>
            <span className="muted small">{c.hint}</span>
          </label>
        ))}
        <div className="muted small">Unchecked ones still show on the seek bar.</div>
      </div>
    </Card>
  );
}

function YtDlp() {
  const [version, setVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    api.ytdlpVersion().then(
      (v) => alive && setVersion(v),
      (e) => alive && setStatus({ ok: false, text: errorText(e) }),
    );
    return () => {
      alive = false;
    };
  }, []);

  const update = async () => {
    setBusy(true);
    setStatus(null);
    try {
      setStatus({ ok: true, text: await api.ytdlpUpdate() });
      setVersion(await api.ytdlpVersion());
    } catch (e) {
      setStatus({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card icon={Package} title="yt-dlp" note="Gets the video streams. When videos stop opening, updating it is the first fix.">
      <Row label="Version" value={version ?? "…"}>
        <button className="btn" onClick={update} disabled={busy}>
          <RefreshCw size={15} className={busy ? "spin" : undefined} />
          {busy ? "Updating…" : "Update"}
        </button>
      </Row>
      {status && <p className={`small settings-status${status.ok ? " muted" : " error"}`}>{status.text}</p>}
    </Card>
  );
}
