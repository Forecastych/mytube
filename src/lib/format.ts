// Formatters are built once: Intl constructors are expensive.
const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
const rel = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

export const views = (n: number | null) => (n == null ? "" : `${compact.format(n)} views`);

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
