// Sign-in with the user's own Google Cloud client. Tokens and the client
// secret stay in Rust (Windows Credential Manager); JS only sees who is
// signed in.
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useSyncExternalStore } from "react";
import { errorText } from "./api";

export type Account = { title: string; avatar: string | null };
export type GoogleStatus = { clientId: string | null; account: Account | null };

type State = {
  status: GoogleStatus | null;
  /** Waiting for the user in the browser. */
  signingIn: boolean;
  busy: boolean;
  error: string | null;
};

let state: State = { status: null, signingIn: false, busy: false, error: null };
const listeners = new Set<() => void>();

function set(patch: Partial<State>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

let loading: Promise<void> | null = null;
export function loadGoogle() {
  if (state.status) return Promise.resolve();
  loading ??= invoke<GoogleStatus>("google_status")
    .then(
      (status) => set({ status }),
      (e) => set({ error: errorText(e) }),
    )
    .finally(() => (loading = null));
  return loading;
}

/** Runs a command that returns the new status; errors land in `state.error`. */
async function run(cmd: string, args?: Record<string, unknown>) {
  set({ busy: true, error: null });
  try {
    set({ status: await invoke<GoogleStatus>(cmd, args) });
    return true;
  } catch (e) {
    set({ error: errorText(e) });
    return false;
  } finally {
    set({ busy: false });
  }
}

export const saveClient = (clientId: string, clientSecret: string) => run("google_set_client", { clientId, clientSecret });
export const signOut = () => run("google_sign_out");
export const forgetClient = () => run("google_forget_client");

export async function signIn() {
  set({ signingIn: true });
  try {
    return await run("google_sign_in");
  } finally {
    set({ signingIn: false });
  }
}

export function cancelSignIn() {
  void invoke("google_sign_in_cancel").catch(() => {});
}

export function useGoogle() {
  const s = useSyncExternalStore(subscribe, () => state);
  useEffect(() => {
    void loadGoogle();
  }, []);
  return s;
}
