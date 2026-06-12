// Non-visual constants (NUMBERS). Every visual constant — color, font,
// duration, type scale — lives in tokens.ts, the single source.

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
