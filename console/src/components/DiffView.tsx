"use client";

import { useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { DiffDocRow, DiffResponse, DiffSection, DiffSharedRow } from "@/lib/api";
import { COLOR, TYPE } from "@/lib/tokens";
import { ExportButton } from "./ExportButton";
import { SensitivityBadge } from "./SensitivityBadge";
import { Skeleton } from "./Skeleton";

/**
 * THE DIFF VIEW (AP-4) — two principals side by side, every difference
 * attributed to the rule responsible. The columns are the service's
 * set-exact partition rendered WHOLE: an empty exclusive column renders as
 * whitespace, because a strict subset is a finding, not an error — no
 * placeholder prose. Divergent-route rows lead the shared table, both rule
 * chips side by side with a hairline between: the two chips disagreeing IS
 * the marker. No icon, no color.
 */
export function DiffView({
  actor,
  left,
  right,
  onClose,
  onOpenDoc,
}: {
  actor: string | null;
  left: string;
  right: string;
  onClose: () => void;
  onOpenDoc: (docId: string) => void;
}) {
  const [diff, setDiff] = useState<DiffResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null) {
      setDiff(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setDiff(null);
    setAvailable(true);
    api
      .getLensDiff(actor, left, right)
      .then((response) => {
        if (!cancelled) {
          setDiff(response);
          setAvailable(response !== null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setDiff(null);
          setAvailable(false);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [actor, left, right]);

  // The console's copy of the priority law (SUBJECT > REBAC > ABAC > AGENT
  // > PUBLIC, lexicographic) — only to pick each side's chip; the service's
  // divergent_route is the authority.
  const ordered =
    diff === null
      ? []
      : [
          ...diff.shared.filter((row) => row.divergent_route),
          ...diff.shared.filter((row) => !row.divergent_route),
        ];

  return (
    <div data-testid="diff-view">
      <div className="mb-3 flex items-center gap-2">
        <h1
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.lg, lineHeight: TYPE.line.display, fontWeight: 600 }}
        >
          Lens diff
        </h1>
        <span className="ml-auto">
          <ExportButton
            actor={actor}
            request={{ view: "diff", diff: { left, right } }}
            filename={
              diff === null
                ? null
                : api.exportFilename("diff", `${left}-${right}`, diff.snapshot_version)
            }
            disabled={loading || diff === null}
          />
        </span>
        <button
          type="button"
          onClick={onClose}
          className="ap-washable ap-soft rounded px-2 py-0.5"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="diff-close"
        >
          Back to lens
        </button>
      </div>

      {loading && (
        <div className="ap-card rounded p-4">
          <Skeleton lines={4} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="diff-unavailable">
          This diff isn&apos;t available.
        </p>
      )}

      {!loading && diff && (
        <>
          {/* Two passports, side by side; the audited line spans both. */}
          <div className="flex flex-wrap items-stretch gap-4">
            <Passport side="left" id={diff.left.id} kind={diff.left.kind} name={diff.left.name} />
            <Passport
              side="right"
              id={diff.right.id}
              kind={diff.right.kind}
              name={diff.right.name}
            />
          </div>
          <p
            className="ap-soft mt-2"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="diff-audited-line"
          >
            Comparing as {diff.actor_id} — this view is audited.
          </p>

          {/* The exclusive columns. EMPTY renders as whitespace — a strict
              subset is a finding, and the absence of prose states it. */}
          <div className="mt-4 flex flex-wrap items-start gap-4">
            <section className="min-w-0 flex-1" data-testid="diff-left-only">
              {diff.left_only.length > 0 && (
                <ExclusiveColumn
                  label={`Only ${diff.left.name}`}
                  sections={diff.left_only}
                  onOpenDoc={onOpenDoc}
                />
              )}
            </section>
            <section className="min-w-0 flex-1" data-testid="diff-right-only">
              {diff.right_only.length > 0 && (
                <ExclusiveColumn
                  label={`Only ${diff.right.name}`}
                  sections={diff.right_only}
                  onOpenDoc={onOpenDoc}
                />
              )}
            </section>
          </div>

          {/* The shared table: divergent routes lead; both chips per row. */}
          {ordered.length > 0 && (
            <section className="ap-card mt-4 rounded p-3" data-testid="diff-shared">
              <h3
                className="ap-soft px-2 pb-1 uppercase tracking-wide"
                style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              >
                Shared
              </h3>
              <ol>
                {ordered.map((row) => (
                  <SharedRowItem key={row.doc.document_id} row={row} onOpenDoc={onOpenDoc} />
                ))}
              </ol>
            </section>
          )}
        </>
      )}
    </div>
  );
}

function Passport({
  side,
  id,
  kind,
  name,
}: {
  side: "left" | "right";
  id: string;
  kind: string;
  name: string;
}) {
  return (
    <section className="ap-card min-w-0 flex-1 rounded p-4" data-testid={`diff-passport-${side}`}>
      <div className="flex flex-wrap items-baseline gap-2">
        <h2
          className="ap-register-chrome"
          style={{ fontSize: TYPE.scale.md, lineHeight: TYPE.line.display, fontWeight: 600 }}
          data-testid={`diff-name-${side}`}
        >
          {name}
        </h2>
        <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.sm }}>
          {id}
        </span>
        <span
          className="ap-hairline ap-register-chrome ap-soft rounded border px-1.5 py-0.5"
          style={{ fontSize: TYPE.scale.xs }}
        >
          {kind}
        </span>
      </div>
    </section>
  );
}

function ExclusiveColumn({
  label,
  sections,
  onOpenDoc,
}: {
  label: string;
  sections: DiffSection[];
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <>
      <h3
        className="ap-soft px-1 pb-1 uppercase tracking-wide"
        style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
      >
        {label}
      </h3>
      <div className="space-y-3">
        {sections.map((section) => (
          <section className="ap-card rounded p-3" key={section.reason}>
            <div className="flex items-baseline justify-between gap-3 px-2 pb-1">
              <h4
                className="ap-register-chrome"
                style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
                data-testid="diff-section-sentence"
              >
                {section.sentence}
              </h4>
              <span
                className="ap-hairline ap-register-evidence ap-soft shrink-0 rounded border px-1.5 py-0.5"
                style={{ fontSize: TYPE.scale.xs }}
                data-testid="diff-section-rule"
              >
                {section.reason}
              </span>
            </div>
            <ol>
              {section.docs.map((doc) => (
                <li key={doc.document_id}>
                  <DocRowButton doc={doc} testid="diff-doc-row" onOpenDoc={onOpenDoc} />
                </li>
              ))}
            </ol>
          </section>
        ))}
      </div>
    </>
  );
}

function DocRowButton({
  doc,
  testid,
  onOpenDoc,
}: {
  doc: DiffDocRow;
  testid: string;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <div className="ap-washable flex w-full items-center gap-3 px-2 py-1.5">
      <button
        type="button"
        onClick={() => onOpenDoc(doc.document_id)}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        data-testid={testid}
      >
        <span className="min-w-0 flex-1">
          <span
            className="ap-register-chrome block truncate"
            style={{
              fontSize: TYPE.scale.sm,
              textDecoration: doc.superseded ? "line-through" : undefined,
              color: doc.superseded ? COLOR.inkSoft : undefined,
            }}
          >
            {doc.title}
          </span>
          <span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {doc.document_id}
          </span>
        </span>
        <SensitivityBadge sensitivity={doc.sensitivity} />
      </button>
      {/* Redaction honored: a missing successor field renders NOTHING. */}
      {doc.superseded && doc.effective_successor && (
        <button
          type="button"
          onClick={() => onOpenDoc(doc.effective_successor!)}
          className="ap-register-evidence ap-affordance-text underline"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="diff-successor-link"
        >
          {doc.effective_successor}
        </button>
      )}
    </div>
  );
}

function classOf(reason: string): number {
  if (reason === "SUBJECT:self") {
    return 0;
  }
  if (reason.startsWith("REBAC:")) {
    return 1;
  }
  if (reason.startsWith("ABAC:")) {
    return 2;
  }
  if (reason.startsWith("AGENT:")) {
    return 3;
  }
  return 4; // PUBLIC
}

function primaryOf(reasons: string[]): string {
  const normalized = reasons.map((r) => (r === "PUBLIC:sensitivity" ? "PUBLIC:all" : r));
  normalized.sort((a, b) => classOf(a) - classOf(b) || a.localeCompare(b));
  return normalized[0] ?? "";
}

function SharedRowItem({
  row,
  onOpenDoc,
}: {
  row: DiffSharedRow;
  onOpenDoc: (docId: string) => void;
}) {
  return (
    <li data-testid="shared-row" data-divergent={String(row.divergent_route)}>
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <DocRowButton doc={row.doc} testid="shared-doc-row" onOpenDoc={onOpenDoc} />
        </div>
        {/* Both rule chips, a hairline between. The chips disagreeing IS
            the divergence marker — no icon, no color. */}
        <span className="flex shrink-0 items-center gap-1.5 pr-2">
          <span
            className="ap-hairline ap-register-evidence ap-soft rounded border px-1 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="shared-chip-left"
          >
            {primaryOf(row.left_reasons)}
          </span>
          <span className="ap-hairline self-stretch border-l" aria-hidden="true" />
          <span
            className="ap-hairline ap-register-evidence ap-soft rounded border px-1 py-0.5"
            style={{ fontSize: TYPE.scale.xs }}
            data-testid="shared-chip-right"
          >
            {primaryOf(row.right_reasons)}
          </span>
        </span>
      </div>
    </li>
  );
}
