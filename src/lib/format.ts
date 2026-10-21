// Formatters are built once: Intl constructors are expensive.
const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
const rel = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

export const views = (n: number | null) => (n == null ? "" : `${compact.format(n)} views`);

/** 75 → "1:15", 3725 → "1:02:05". */
export function clock(sec: number): string {
  const s = Math.max(0, Math.floor(sec || 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

const steps: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

export function ago(unixSec: number): string {
  const diff = unixSec - Date.now() / 1000;
  for (const [unit, sec] of steps) {
    if (Math.abs(diff) >= sec) return rel.format(Math.round(diff / sec), unit);
  }
  return "just now";
}
