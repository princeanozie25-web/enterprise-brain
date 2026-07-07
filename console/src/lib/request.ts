// K3 — THE TYPED-FETCH SEAM.
//
// Every request to the Ask Brain service (SERVICE_URL) routes through
// `serviceRequest` below: one abort/timeout discipline, one error taxonomy,
// no retries, no queuing — one request, one outcome. api.ts is the only
// module that calls it; K3-2 greps the built source to prove no bare
// fetch() to SERVICE_URL survives outside this file.
//
// LEDGER IS NOT ON THIS SEAM. ledger.ts (LEDGER_URL) is a wholly separate
// trust boundary: the Bursar producer is a read-only contract consumer and
// NO session bearer may EVER ride to it. Its 3,000ms timeout, zero
// credentials, and single-shot behavior are law and stay byte-identical — it
// is deliberately excluded so the two boundaries can never be merged.

/** Abort budgets (ms). POST /ask covers the server's 15s generation budget
 * plus retrieval + per-sentence grounding, with margin; everything else is a
 * metadata read. Named constants, never inline. */
export const ASK_TIMEOUT_MS = 20_000;
export const DEFAULT_TIMEOUT_MS = 8_000;

/** The one error taxonomy every seam call resolves to. A discriminated union
 * so callers pattern-match exhaustively — no bare Error, no thrown strings. */
export type RequestError =
  | { kind: "unauthorized" }
  | { kind: "timeout" }
  | { kind: "network" }
  | { kind: "service"; status: number; body: string }
  | { kind: "invalid_body" };

/** Thrown by the seam; carries the taxonomy. Callers catch and switch on
 * `.error.kind` (api.ts maps a few to null/typed sentinels where a route's
 * contract already defines one, e.g. 404 → null for /doc). */
export class ServiceRequestError extends Error {
  readonly error: RequestError;
  constructor(error: RequestError) {
    super(error.kind);
    this.name = "ServiceRequestError";
    this.error = error;
  }
}

/** Routes chosen for the longer budget: POST /ask only (generation path). */
function timeoutFor(path: string, init: RequestInit): number {
  const method = (init.method ?? "GET").toUpperCase();
  if (method === "POST" && path === "/ask") return ASK_TIMEOUT_MS;
  return DEFAULT_TIMEOUT_MS;
}

/**
 * The request core. `path` is service-relative (e.g. "/graph"); `base` is
 * SERVICE_URL, passed in so this module imports no app constants and stays
 * trivially testable. Returns the raw Response on 2xx AND on non-2xx status
 * codes a caller's contract still needs to read (404, 400) — status handling
 * that is uniform (401 → unauthorized) is done HERE; status handling that is
 * route-specific (404 → null) is left to the caller via the returned Response.
 *
 * Throws ServiceRequestError for: 401 (unauthorized), abort (timeout),
 * fetch rejection (network). It never throws for other HTTP status — the
 * Response comes back so the caller can branch. `parseJson` below is the
 * companion that turns a body into a typed value or an invalid_body error.
 */
export async function serviceRequest(
  base: string,
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  const controller = new AbortController();
  const budget = timeoutFor(path, init);
  const timer = setTimeout(() => controller.abort(), budget);
  let response: Response;
  try {
    response = await fetch(`${base}${path}`, { ...init, signal: controller.signal });
  } catch (cause) {
    // AbortController.abort() surfaces as an AbortError DOMException.
    if (cause instanceof DOMException && cause.name === "AbortError") {
      throw new ServiceRequestError({ kind: "timeout" });
    }
    throw new ServiceRequestError({ kind: "network" });
  } finally {
    clearTimeout(timer);
  }
  // Uniform status: an expired/invalid session is one taxonomy branch for
  // every route (Track 2 handles it once at the app edge).
  if (response.status === 401) {
    throw new ServiceRequestError({ kind: "unauthorized" });
  }
  return response;
}

/**
 * Reads a 2xx JSON body as T, or throws { kind: 'invalid_body' } if it does
 * not parse. Non-2xx bodies the caller did not pre-handle become
 * { kind: 'service', status, body } — a stated service state, never a silent
 * empty. Callers that treat a specific status as a typed sentinel (404 → null)
 * MUST check `response.status` before calling this.
 */
export async function parseJson<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new ServiceRequestError({ kind: "service", status: response.status, body });
  }
  try {
    return (await response.json()) as T;
  } catch {
    throw new ServiceRequestError({ kind: "invalid_body" });
  }
}

/** True when a caught value is a seam error of the given kind — the ergonomic
 * check for the app edge (Track 2 tests `isRequestError(e, "unauthorized")`). */
export function isRequestError(value: unknown, kind?: RequestError["kind"]): value is ServiceRequestError {
  return value instanceof ServiceRequestError && (kind === undefined || value.error.kind === kind);
}
