import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { errorText } from "../lib/api";
import { addFavorite } from "../lib/favorites";
import "./AddChannel.css";

export function AddChannel({ autoFocus }: { autoFocus?: boolean }) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!input.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await addFavorite(input);
      setInput("");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="add-channel" onSubmit={submit}>
      <div className="add-channel-row">
        <input
          className="field"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Channel link or @handle"
          spellCheck={false}
          autoFocus={autoFocus}
          disabled={busy}
        />
        <button className="btn btn-primary" disabled={busy || !input.trim()}>
          <Plus size={16} />
          {busy ? "Adding…" : "Add to favorites"}
        </button>
      </div>
      {error && <div className="form-error">{error}</div>}
    </form>
  );
}
