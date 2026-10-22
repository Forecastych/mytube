// Building blocks of the Settings screen, shared by its sections.
import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

export function Card({ icon: Icon, title, note, children }: { icon: LucideIcon; title: string; note?: string; children: ReactNode }) {
  return (
    <section className="settings-card">
      <div className="settings-card-head">
        <Icon size={20} className="accent" />
        <div>
          <h2>{title}</h2>
          {note && <p className="muted small">{note}</p>}
        </div>
      </div>
      {children}
    </section>
  );
}

export function Row({
  label,
  hint,
  value,
  disabled,
  children,
}: {
  label: string;
  hint?: string;
  value?: string;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={`settings-row${disabled ? " disabled" : ""}`}>
      <div className="settings-row-text">
        <div>
          {label}
          {value && <span className="muted settings-value">{value}</span>}
        </div>
        {hint && <div className="muted small">{hint}</div>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}
