// A screen that throws while rendering shows this instead of blanking the
// whole window. Sidebar and header stay, so the user can just go elsewhere.
import { Component, type ErrorInfo, type ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { RotateCw } from "lucide-react";

type State = { error: Error | null };

class Boundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("screen crashed:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    // A lazy screen that fails to load is usually a stale chunk after an update.
    const chunk = /dynamically imported module|Importing a module script failed/i.test(error.message);
    return (
      <section className="placeholder">
        <h1>Something went wrong</h1>
        <p>{chunk ? "This screen didn't load." : error.message}</p>
        <div className="placeholder-actions">
          {!chunk && (
            <button className="btn" onClick={() => this.setState({ error: null })}>
              Try again
            </button>
          )}
          <button className="btn" onClick={() => location.reload()}>
            <RotateCw size={16} /> Reload app
          </button>
        </div>
      </section>
    );
  }
}

/** Resets on navigation: another page gets a fresh start. */
export function ScreenBoundary({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  return <Boundary key={pathname}>{children}</Boundary>;
}
