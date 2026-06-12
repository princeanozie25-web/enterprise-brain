// All magic numbers in one place (NUMBERS, M3b).

/** The Ask Brain service — loopback only, always. */
export const SERVICE_URL = "http://127.0.0.1:8787";

/** Doc/sealed-context snippet cap (chars), mirrored from the service. */
export const SNIPPET_CAP = 480;

/** Citation chip format: [d0123]. */
export const CITATION_PATTERN = /\[([A-Za-z0-9_.-]+)\]/g;

/** Doc inspector side-sheet width (px). */
export const INSPECTOR_WIDTH = 420;

/** Principal switcher virtualization (124 demo principals). */
export const PRINCIPAL_ROW_HEIGHT = 32;
export const PRINCIPAL_LIST_HEIGHT = 288;
export const VIRTUAL_OVERSCAN = 4;

/**
 * The five sensitivity levels with a fixed, labeled, colorblind-safe scale
 * (Okabe–Ito palette). Labels always accompany color — color is never the
 * only signal.
 */
export const SENSITIVITY_SCALE: Record<
  string,
  { label: string; background: string; border: string }
> = {
  public: { label: "public", background: "#E8F1F8", border: "#0072B2" },
  internal: { label: "internal", background: "#E6F4EF", border: "#009E73" },
  confidential: { label: "confidential", background: "#FBF1DC", border: "#E69F00" },
  restricted: { label: "restricted", background: "#F9E8DE", border: "#D55E00" },
  special_category: { label: "special category", background: "#F6EAF1", border: "#CC79A7" },
};
