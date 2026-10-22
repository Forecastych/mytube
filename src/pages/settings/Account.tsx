// Settings → Account: the user's own Google Cloud client, then sign-in.
import { useState, type FormEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, LogIn, LogOut, UserRound } from "lucide-react";
import { cancelSignIn, forgetClient, saveClient, signIn, signOut, useGoogle } from "../../lib/google";
import { Card, Row } from "./parts";

const CONSOLE = "https://console.cloud.google.com/";

/** `123456-abc.apps.googleusercontent.com` → `123456-…`: enough to tell projects apart. */
const shortId = (id: string) => `${id.split("-")[0]}-…`;

export function Account() {
  const { status, signingIn, busy, error } = useGoogle();
  const [editing, setEditing] = useState(false);

  const note = "Optional. Imports your subscriptions and opens Trending. Read-only: MyTube never posts, likes or subscribes.";
  if (!status) {
    return (
      <Card icon={UserRound} title="Account" note={note}>
        {error && <p className="small settings-status error">{error}</p>}
      </Card>
    );
  }

  const account = status.account;
  return (
    <Card icon={UserRound} title="Account" note={note}>
      {account ? (
        <div className="settings-row">
          {account.avatar ? (
            <img className="account-avatar" src={account.avatar} alt="" decoding="async" />
          ) : (
            <span className="account-avatar" />
          )}
          <div className="settings-row-text">
            <div>{account.title}</div>
            <div className="muted small">Signed in with Google</div>
          </div>
          <button className="btn" onClick={() => void signOut()} disabled={busy}>
            <LogOut size={15} /> Sign out
          </button>
        </div>
      ) : status.clientId && !editing ? (
        <Row
          label="Google account"
          hint={signingIn ? "Finish signing in in your browser, then come back here." : "Opens your browser. Google asks once whether MyTube may read your YouTube."}
        >
          {signingIn ? (
            <button className="btn" onClick={cancelSignIn}>
              Cancel
            </button>
          ) : (
            <button className="btn btn-primary" onClick={() => void signIn()} disabled={busy}>
              <LogIn size={15} /> Sign in with Google
            </button>
          )}
        </Row>
      ) : (
        <ClientForm onDone={() => setEditing(false)} onCancel={status.clientId ? () => setEditing(false) : undefined} />
      )}

      {status.clientId && !editing && (
        <Row label="Google Cloud client" value={shortId(status.clientId)} hint="Your own project: its quota is yours alone.">
          <div className="account-actions">
            <button className="btn" onClick={() => setEditing(true)} disabled={busy || signingIn}>
              Change
            </button>
            <button
              className="btn"
              title="Signs out and removes the Client ID and secret from this computer"
              onClick={() => void forgetClient()}
              disabled={busy || signingIn}
            >
              Remove
            </button>
          </div>
        </Row>
      )}
      {error && <p className="small settings-status error">{error}</p>}
    </Card>
  );
}

function ClientForm({ onDone, onCancel }: { onDone: () => void; onCancel?: () => void }) {
  const { busy } = useGoogle();
  const [id, setId] = useState("");
  const [secret, setSecret] = useState("");

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (await saveClient(id, secret)) onDone();
  }

  return (
    <form className="account-form" onSubmit={submit}>
      <ol className="account-steps muted small">
        <li>
          Open{" "}
          <button type="button" className="link" onClick={() => void openUrl(CONSOLE).catch(() => {})}>
            Google Cloud Console <ExternalLink size={12} />
          </button>{" "}
          and create a project (free, no card needed).
        </li>
        <li>APIs &amp; Services → Library → <b>YouTube Data API v3</b> → Enable.</li>
        <li>
          Google Auth Platform → Branding: any app name and your email. Audience: <b>External</b>, then <b>Publish app</b>.
          In Testing mode Google signs you out every 7 days.
        </li>
        <li>
          Clients → Create client → Application type <b>Desktop app</b>. Paste its Client ID and secret below.
        </li>
        <li>
          When signing in, Google warns the app isn't verified: it's your own app, click Advanced → Go to … (unsafe).
        </li>
      </ol>
      <input
        className="field"
        value={id}
        onChange={(e) => setId(e.target.value)}
        placeholder="Client ID: ….apps.googleusercontent.com"
        spellCheck={false}
        autoComplete="off"
        disabled={busy}
      />
      <input
        className="field"
        type="password"
        value={secret}
        onChange={(e) => setSecret(e.target.value)}
        placeholder="Client secret"
        spellCheck={false}
        autoComplete="off"
        disabled={busy}
      />
      <p className="muted small">Stored in Windows Credential Manager, never in a file.</p>
      <div className="account-actions">
        <button className="btn btn-primary" disabled={busy || !id.trim() || !secret.trim()}>
          Save
        </button>
        {onCancel && (
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}
