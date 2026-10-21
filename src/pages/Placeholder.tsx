import "./Placeholder.css";

export function Placeholder({ title, note }: { title: string; note: string }) {
  return (
    <section className="placeholder">
      <h1>{title}</h1>
      <p>{note}</p>
    </section>
  );
}
