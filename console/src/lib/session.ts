// FC-A1 (AUTH-1): the console's session.
//
// Identity is no longer a header the client asserts. The client logs in once
// (POST /auth/login — demo path: no credential), the server mints a signed
// session, and every request carries that session's bearer token. Switching
// Work Identity = logging in as that principal (smooth admin view-as is AUTH-3,
// not this slice). The non-dismissible Demo Identity banner stays.

import { SERVICE_URL } from "./constants";
import { serviceRequest } from "./request";

const STORAGE_KEY = "eb_session";
const RETURN_KEY = "eb_return_intent";

type Session = { principal_id: string; token: string; expires_at: number };

/** K3 Track 2: where the person was when a session expired, so re-picking an
 * identity can restore the room + the staged (never auto-submitted) query.
 * `principal` records WHO staged it — the query is content that belongs to
 * that identity's scope, so it is restored only when the SAME identity is
 * re-picked (a different pick starts that identity fresh). */
export type ReturnIntent = { path: string; query: string | null; principal: string | null };

let current: Session | null = null;

function load(): Session | null {
  if (current) return current;
  if (typeof sessionStorage === "undefined") return null;
  const raw = sessionStorage.getItem(STORAGE_KEY);
  if (!raw) return null;
  try {
    current = JSON.parse(raw) as Session;
  } catch {
    current = null;
  }
  return current;
}

function store(session: Session | null): void {
  current = session;
  if (typeof sessionStorage === "undefined") return;
  if (session) sessionStorage.setItem(STORAGE_KEY, JSON.stringify(session));
  else sessionStorage.removeItem(STORAGE_KEY);
}

/** The principal of the active session, or null. */
export function sessionPrincipal(): string | null {
  return load()?.principal_id ?? null;
}

/** The Authorization header for the active session (empty object if none). */
export function authHeader(): Record<string, string> {
  const session = load();
  return session ? { authorization: `Bearer ${session.token}` } : {};
}

/** The active session's bearer token, or null. Callers capture this at
 * request-issue time so a late 401 can be attributed to the token that
 * actually made the call (see notifyExpiry). */
export function currentToken(): string | null {
  return load()?.token ?? null;
}

/**
 * Mint a server session for `principalId` (demo path: no credential).
 * Idempotent — re-logging-in as the already-active principal is a no-op.
 * Throws on failure so callers can surface a signed-out state. Routes through
 * the typed-fetch seam (K3-2: no bare fetch to the service), but a failed
 * LOGIN is not an expiry — this path never fires the expiry notifier.
 */
export async function loginAs(principalId: string): Promise<void> {
  const active = load();
  if (active && active.principal_id === principalId) return;
  const response = await serviceRequest(SERVICE_URL, "/auth/login", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ principal_id: principalId }),
  });
  if (!response.ok) {
    throw new Error(`login failed: ${response.status}`);
  }
  const data = (await response.json()) as {
    principal_id: string;
    session_token: string;
    expires_at: number;
  };
  store({
    principal_id: data.principal_id,
    token: data.session_token,
    expires_at: data.expires_at,
  });
}

/** Revoke the active session (best-effort) and clear it locally. */
export async function logout(): Promise<void> {
  const session = load();
  if (session) {
    try {
      await serviceRequest(SERVICE_URL, "/auth/logout", {
        method: "POST",
        headers: { authorization: `Bearer ${session.token}` },
      });
    } catch {
      // Best-effort: clear locally regardless.
    }
  }
  store(null);
}

// ---------------------------------------------------------------------------
// K3 Track 2 — SESSION EXPIRY. One expiry, one human action, one restore:
// no auto-retry, no token refresh, no silent re-mint. The seam layer calls
// `notifyExpiry()` on { kind: 'unauthorized' }; the app edge (Console)
// subscribes, captures return intent, and routes to the identity picker.
// ---------------------------------------------------------------------------

type ExpiryListener = () => void;
const expiryListeners = new Set<ExpiryListener>();

/** Subscribe to session expiry. Returns an unsubscribe function. */
export function onSessionExpired(listener: ExpiryListener): () => void {
  expiryListeners.add(listener);
  return () => {
    expiryListeners.delete(listener);
  };
}

/** Fired by the seam on a 401 from any service call: clears the local session
 * and notifies subscribers exactly once. Idempotent when already cleared.
 *
 * `issuedToken` is the bearer the failing request actually carried (captured
 * at issue time). A 401 is a real expiry ONLY when it belongs to the
 * currently-active session — if a newer session has since been minted (an
 * identity switch completed while an old request was in flight), the stale
 * 401 is IGNORED so it can't destroy the fresh, valid session or manufacture
 * a spurious expiry. Omitting `issuedToken` keeps the old unconditional
 * behavior (used where no token was in play). */
export function notifyExpiry(issuedToken?: string): void {
  const active = load();
  if (issuedToken !== undefined && active !== null && active.token !== issuedToken) {
    // A 401 from a superseded session — a newer valid one is active. Ignore.
    return;
  }
  const wasActive = active !== null;
  store(null);
  if (!wasActive) return;
  for (const listener of expiryListeners) listener();
}

/** Stash where to return after re-authenticating (room path + staged query). */
export function captureReturnIntent(intent: ReturnIntent): void {
  if (typeof sessionStorage === "undefined") return;
  sessionStorage.setItem(RETURN_KEY, JSON.stringify(intent));
}

/** Read and clear the stashed return intent (null if none). */
export function takeReturnIntent(): ReturnIntent | null {
  if (typeof sessionStorage === "undefined") return null;
  const raw = sessionStorage.getItem(RETURN_KEY);
  if (!raw) return null;
  sessionStorage.removeItem(RETURN_KEY);
  try {
    return JSON.parse(raw) as ReturnIntent;
  } catch {
    return null;
  }
}
