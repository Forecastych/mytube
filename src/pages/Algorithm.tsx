import { useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  Ban,
  Clock,
  EyeOff,
  FileText,
  Film,
  Info,
  Link2Off,
  Plus,
  Scale,
  SlidersHorizontal,
  Smartphone,
  Sparkles,
  UserRound,
  X,
  type LucideIcon,
} from "lucide-react";
import { Switch } from "../components/Switch";
import { errorText, thumb } from "../lib/api";
import {
  blockChannel,
  rebalance,
  SOURCES,
  updateRules,
  useAlgorithm,
  type FeedVideo,
  type Rules,
  type Source,
  type Topic,
} from "../lib/algorithm";
import "./Algorithm.css";

/** The length "Hide videos under N min" uses. */
const MIN_MINUTES = 3;

export default function Algorithm() {
  const { rules, topics, preview, saving, error } = useAlgorithm();

  if (!rules) {
    return error ? <div className="form-error">{error}</div> : null;
  }

  return (
    <div className="algo">
      <header className="algo-head">
        <SlidersHorizontal size={28} className="accent" />
        <div>
          <h1>Algorithm</h1>
          <p className="muted">Your feed follows only these rules. Nothing is guessed from what you watch.</p>
        </div>
        <span className="muted small algo-status">{saving ? "Saving…" : error ? "" : "Saved"}</span>
      </header>
      {error && <div className="form-error">{error}</div>}

      <div className="algo-cols">
        <div className="algo-main">
          <TopicsCard rules={rules} topics={topics} />
          <WordsCard rules={rules} />
          <ChannelsCard rules={rules} />
          <VideosCard rules={rules} />
          <MixCard rules={rules} />
        </div>

        <aside className="algo-side">
          <section className="algo-card">
            <div className="algo-card-head">
              <Sparkles size={22} className="accent" />
              <div>
                <h2>Your feed, your rules</h2>
                <p className="muted small">Top of your favorites feed with these rules.</p>
              </div>
            </div>
            <Preview videos={preview.videos} hidden={preview.hidden} />
          </section>

          <Toggle
            icon={Smartphone}
            title="Hide Shorts"
            note="Vertical clips won't appear in your feed."
            checked={rules.hideShorts}
            onChange={(v) => updateRules({ hideShorts: v })}
          />
          <Toggle
            icon={Clock}
            title={`Hide videos under ${MIN_MINUTES} min`}
            note="Works where the length is known: trending and subscriptions. Channel RSS doesn't carry it."
            checked={rules.minMinutes > 0}
            onChange={(v) => updateRules({ minMinutes: v ? MIN_MINUTES : 0 })}
          />
          <Toggle
            icon={EyeOff}
            title="Hide already watched"
            note="Videos you've opened won't come back."
            checked={rules.hideWatched}
            onChange={(v) => updateRules({ hideWatched: v })}
          />
          <Toggle
            icon={Link2Off}
            title="Watching a video does NOT change recommendations"
            note="Your feed changes only when you change these rules. Always on for now."
            checked={!rules.watchAffects}
            onChange={(v) => updateRules({ watchAffects: !v })}
            disabled
          />

          <div className="algo-tip">
            <Info size={18} className="accent" />
            <p className="muted small">
              Blocking is instant and reversible: remove a chip and its videos come back.
            </p>
          </div>
        </aside>
      </div>
    </div>
  );
}

function Card({ icon: Icon, title, note, action, children }: {
  icon: LucideIcon;
  title: string;
  note: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="algo-card">
      <div className="algo-card-head">
        <Icon size={22} className="muted" />
        <div>
          <h2>{title}</h2>
          <p className="muted small">{note}</p>
        </div>
        {action && <div className="section-action">{action}</div>}
      </div>
      {children}
    </section>
  );
}

function Chips({ items, label, onRemove }: { items: string[]; label: (s: string) => string; onRemove: (s: string) => void }) {
  if (items.length === 0) return null;
  return (
    <ul className="algo-chips">
      {items.map((s) => (
        <li key={s} className="algo-chip">
          <span>{label(s)}</span>
          <button className="algo-x" title={`Remove ${label(s)}`} onClick={() => onRemove(s)}>
            <X size={14} />
          </button>
        </li>
      ))}
    </ul>
  );
}

function TopicsCard({ rules, topics }: { rules: Rules; topics: Topic[] }) {
  const [open, setOpen] = useState(false);
  const labels = new Map(topics.map((t) => [t.id, t.label]));
  const label = (id: string) => labels.get(id) ?? id;
  const add = (id: string) => {
    if (!rules.blockedTopics.includes(id)) updateRules({ blockedTopics: [...rules.blockedTopics, id] });
  };

  return (
    <Card
      icon={Ban}
      title="Blocked topics"
      note="Hidden by category and by typical title words, in English and Russian."
      action={
        <div className="algo-pop-anchor">
          <button className="btn btn-outline" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
            <Plus size={16} /> Add topic
          </button>
          {open && <TopicPicker topics={topics} blocked={rules.blockedTopics} onPick={add} onClose={() => setOpen(false)} />}
        </div>
      }
    >
      <Chips
        items={rules.blockedTopics}
        label={label}
        onRemove={(id) => updateRules({ blockedTopics: rules.blockedTopics.filter((t) => t !== id) })}
      />
      {rules.blockedTopics.length === 0 && <p className="muted small">No topics blocked.</p>}
    </Card>
  );
}

function TopicPicker({ topics, blocked, onPick, onClose }: {
  topics: Topic[];
  blocked: string[];
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [custom, setCustom] = useState("");
  const free = topics.filter((t) => !blocked.includes(t.id));

  useEffect(() => {
    const down = (e: MouseEvent) => {
      // The anchor holds the "Add topic" button too: its own click toggles.
      if (!ref.current?.parentElement?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    // Listeners exist only while the menu is open.
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);

  function submit(e: FormEvent) {
    e.preventDefault();
    const s = custom.trim();
    if (!s) return;
    onPick(s);
    setCustom("");
    onClose();
  }

  return (
    <div className="algo-pop glass" ref={ref}>
      <div className="algo-pop-list">
        {free.map((t) => (
          <button key={t.id} className="algo-pop-item" onClick={() => onPick(t.id)}>
            {t.label}
          </button>
        ))}
        {free.length === 0 && <span className="muted small">All built-in topics are blocked.</span>}
      </div>
      <form onSubmit={submit}>
        <input
          className="field"
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          placeholder="Other topic, e.g. fishing"
          autoFocus
          spellCheck={false}
        />
      </form>
    </div>
  );
}

function WordsCard({ rules }: { rules: Rules }) {
  const [word, setWord] = useState("");

  function submit(e: FormEvent) {
    e.preventDefault();
    const w = word.trim().toLowerCase();
    if (!w) return;
    if (!rules.blockedWords.includes(w)) updateRules({ blockedWords: [...rules.blockedWords, w] });
    setWord("");
  }

  return (
    <Card
      icon={FileText}
      title="Blocked words in titles"
      note="Whole words, any case. End with * to catch word endings: влог* also hides влоги."
    >
      <form className="algo-row" onSubmit={submit}>
        <input
          className="field"
          value={word}
          onChange={(e) => setWord(e.target.value)}
          placeholder="Add a word or phrase…"
          spellCheck={false}
        />
        <button className="btn btn-primary" disabled={!word.trim()}>
          <Plus size={16} /> Add
        </button>
      </form>
      <Chips
        items={rules.blockedWords}
        label={(w) => w}
        onRemove={(w) => updateRules({ blockedWords: rules.blockedWords.filter((x) => x !== w) })}
      />
    </Card>
  );
}

function ChannelsCard({ rules }: { rules: Rules }) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await blockChannel(input);
      setInput("");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card icon={UserRound} title="Blocked channels" note="Nothing from these channels, anywhere in the feed.">
      <form className="algo-row" onSubmit={submit}>
        <input
          className="field"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Channel link or @handle"
          spellCheck={false}
          disabled={busy}
        />
        <button className="btn btn-outline" disabled={busy || !input.trim()}>
          <Plus size={16} /> {busy ? "Looking up…" : "Add channel"}
        </button>
      </form>
      {error && <div className="form-error">{error}</div>}
      {rules.blockedChannels.length > 0 && (
        <ul className="algo-channels">
          {rules.blockedChannels.map((c) => (
            <li key={c.id}>
              {c.avatar ? <img src={c.avatar} alt="" loading="lazy" decoding="async" /> : <span className="algo-avatar" />}
              <span>{c.title}</span>
              <button
                className="algo-x"
                title={`Unblock ${c.title}`}
                onClick={() => updateRules({ blockedChannels: rules.blockedChannels.filter((x) => x.id !== c.id) })}
              >
                <X size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/** Shown before "Show all": the list can reach a thousand entries. */
const VIDEOS_SHOWN = 20;

function VideosCard({ rules }: { rules: Rules }) {
  const [all, setAll] = useState(false);
  const list = rules.blockedVideos;
  // Stored oldest first; the one just hidden is the one people look for.
  const newest = list.slice().reverse();
  const shown = all ? newest : newest.slice(0, VIDEOS_SHOWN);

  return (
    <Card icon={Film} title="Hidden videos" note="Right-click a video card → Hide this video. It won't show up again.">
      {list.length === 0 ? (
        <p className="muted small">Nothing hidden yet.</p>
      ) : (
        <ul className="algo-channels algo-videos">
          {shown.map((v) => (
            <li key={v.id}>
              <img src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
              <span>{v.title || v.id}</span>
              <button
                className="algo-x"
                title={`Show ${v.title || "this video"} again`}
                onClick={() => updateRules({ blockedVideos: list.filter((x) => x.id !== v.id) })}
              >
                <X size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}
      {!all && list.length > VIDEOS_SHOWN && (
        <button className="btn algo-more" onClick={() => setAll(true)}>
          Show all {list.length}
        </button>
      )}
    </Card>
  );
}

const sourceInfo: Record<Source, { label: string; note?: string }> = {
  favorites: { label: "Favorites" },
  subscriptions: { label: "Subscriptions", note: "after sign-in" },
  trending: { label: "Trending", note: "after sign-in" },
  related: { label: "Related to favorites", note: "coming soon" },
};

function MixCard({ rules }: { rules: Rules }) {
  return (
    <Card icon={Scale} title="Feed mix" note="Share of each source in the Home feed. Always adds up to 100%.">
      <ul className="algo-mix">
        {SOURCES.map((s) => (
          <li key={s}>
            <span className={`algo-dot src-${s}`} />
            <span className="algo-mix-label">
              {sourceInfo[s].label}
              {sourceInfo[s].note && <span className="muted small"> · {sourceInfo[s].note}</span>}
            </span>
            <input
              type="range"
              min={0}
              max={100}
              value={rules.mix[s]}
              style={{ "--p": `${rules.mix[s]}%` } as CSSProperties}
              aria-label={`${sourceInfo[s].label} share`}
              onChange={(e) => updateRules({ mix: rebalance(rules.mix, s, Number(e.target.value)) })}
            />
            <span className="algo-pct">{rules.mix[s]}%</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Toggle({ icon: Icon, title, note, checked, onChange, disabled }: {
  icon: LucideIcon;
  title: string;
  note: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <section className="algo-card algo-toggle">
      <Icon size={22} className="muted" />
      <div>
        <h3>{title}</h3>
        <p className="muted small">{note}</p>
      </div>
      <Switch checked={checked} onChange={onChange} label={title} disabled={disabled} />
    </section>
  );
}

function Preview({ videos, hidden }: { videos: FeedVideo[]; hidden: number }) {
  const [first, ...rest] = videos;
  if (!first) {
    return (
      <p className="muted small algo-preview-empty">
        {hidden > 0 ? "Your rules hide everything cached so far." : "Add favorite channels to see a preview."}
      </p>
    );
  }
  return (
    <div className="algo-preview">
      <Link to={`/watch/${first.id}`} className="algo-preview-top" title={first.title}>
        <img src={thumb(first.id)} alt="" decoding="async" />
        <div>
          <div className="algo-preview-title">{first.title}</div>
          <div className="muted small">{first.channelTitle}</div>
        </div>
      </Link>
      {rest.length > 0 && (
        <div className="algo-preview-row">
          {rest.map((v) => (
            <Link key={v.id} to={`/watch/${v.id}`} title={v.title}>
              <img src={thumb(v.id)} alt="" loading="lazy" decoding="async" />
            </Link>
          ))}
        </div>
      )}
      {hidden > 0 && (
        <p className="muted small">
          {hidden} video{hidden > 1 ? "s" : ""} hidden by your rules
        </p>
      )}
    </div>
  );
}
