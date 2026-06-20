// FC-A1 (AUTH-1): the console's session.
//
// Identity is no longer a header the client asserts. The client logs in once
// (POST /auth/login — demo path: no credential), the server mints a signed
// session, and every request carries that session's bearer token. Switching
// Work Identity = logging in as that principal (smooth admin view-as is AUTH-3,
// not this slice). The non-dismissible Demo Identity banner stays.

import { SERVICE_URL } from "./constants";

const STORAGE_KEY = "eb_session";

type Session = { principal_id: string; token: string; expires_at: number };

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

/**
 * Mint a server session for `principalId` (demo path: no credential).
 * Idempotent — re-logging-in as the already-active principal is a no-op.
 * Throws on failure so callers can surface a signed-out state.
 */
export async function loginAs(principalId: string): Promise<void> {
  const active = load();
  if (active && active.principal_id === principalId) return;
  const response = await fetch(`${SERVICE_URL}/auth/login`, {
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
      await fetch(`${SERVICE_URL}/auth/logout`, {
        method: "POST",
        headers: { authorization: `Bearer ${session.token}` },
      });
    } catch {
      // Best-effort: clear locally regardless.
    }
  }
  store(null);
}
