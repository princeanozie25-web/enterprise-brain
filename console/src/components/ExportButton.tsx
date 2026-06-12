"use client";

import { useCallback, useState } from "react";
import * as api from "@/lib/api";
import type { ExportRequest } from "@/lib/api";
import { TYPE } from "@/lib/tokens";

/**
 * AP-5: the one quiet affordance, identical anatomy in its four homes
 * (Lens room header, Diff header, Atlas capability sheet, Ask answer
 * card). Chrome register, ink-soft, no icon — none was needed (flagged in
 * the AP-5 closeout). Disabled is a STATE, not a hiding place, and carries
 * no explanatory prose: the absence law extends to tooltips.
 *
 * The click sends PARAMETERS naming the current view — never content — and
 * downloads the server-derived, attested PDF.
 */
export function ExportButton({
  actor,
  request,
  filename,
  disabled = false,
}: {
  actor: string | null;
  /** null while the view has nothing exportable (loading, degraded-empty). */
  request: ExportRequest | null;
  filename: string | null;
  disabled?: boolean;
}) {
  const [busy, setBusy] = useState(false);

  const onExport = useCallback(async () => {
    if (actor === null || request === null || filename === null || busy) {
      return;
    }
    setBusy(true);
    try {
      const blob = await api.exportEvidence(actor, request);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch {
      // The service refused or failed. The affordance stays quiet — the
      // service never explains absence and neither does the console.
    } finally {
      setBusy(false);
    }
  }, [actor, request, filename, busy]);

  return (
    <button
      type="button"
      onClick={onExport}
      disabled={disabled || busy || actor === null || request === null || filename === null}
      className="ap-washable ap-register-chrome ap-soft rounded px-2 py-0.5"
      style={{ fontSize: TYPE.scale.xs }}
      data-testid="export-evidence"
    >
      Export evidence
    </button>
  );
}
