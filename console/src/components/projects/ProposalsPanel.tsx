"use client";

import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type {
  ProposalBoxView,
  WorkflowProposal,
  WorkflowProposalEmpty,
} from "@/lib/api";
import { isRequestError } from "@/lib/request";
import { TYPE } from "@/lib/tokens";
import { MotionSection } from "../MotionPrimitives";
import { PersonAvatar } from "../PersonAvatar";
import { Skeleton } from "../Skeleton";

/**
 * SHOWCASE-III (Showreel Track B) — grounded workflow proposals, the console
 * face of EB's first mutation path. A model DRAFTS a staged plan whose every
 * step is verbatim-anchored to a document the proposer can already see; the
 * draft renders here watermarked "Proposed"; ONLY the resolved approver's
 * decision (server-audited before any effect) materializes it into real
 * pipeline work. Every state is honest: generation that cannot be grounded
 * says so and writes nothing; anchors a viewer is not cleared for are
 * withheld markers, never content (the S4 no-laundering law, applied
 * server-side — this panel just renders what crossed).
 */

const REFUSAL_NOTE = "could not be grounded in your sources and were dropped";

function statusChip(p: WorkflowProposal): string {
  if (p.status === "pending") return "Proposed";
  if (p.status === "approved") return p.materialized ? "Approved · materialized" : "Approved";
  if (p.status === "denied") return "Denied";
  return p.status;
}

function AnchorChips({ box }: { box: ProposalBoxView }) {
  return (
    <span className="mt-1.5 flex flex-wrap items-center gap-1.5">
      {box.anchors.map((anchor, i) =>
        anchor.visible && anchor.doc_id ? (
          <span
            key={`${box.box_index}:${i}`}
            className="ap-chip ap-register-evidence rounded-lg px-1.5 py-0.5"
            data-testid="proposal-anchor-chip"
            title={anchor.quote ?? undefined}
          >
            {anchor.doc_id}
          </span>
        ) : (
          <span
            key={`${box.box_index}:${i}`}
            className="ap-chip rounded-lg px-1.5 py-0.5"
            data-testid="proposal-anchor-withheld"
            style={{ fontStyle: "italic" }}
          >
            source outside your view
          </span>
        ),
      )}
    </span>
  );
}

function ProposalBox({ box }: { box: ProposalBoxView }) {
  const visibleQuote = box.anchors.find((a) => a.visible && a.quote)?.quote ?? null;
  return (
    <li className="ap-card rounded-lg border p-3" data-testid="proposal-box">
      <p className="ap-soft uppercase tracking-wide" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
        Step {box.box_index + 1} · {box.stage}
      </p>
      <p className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
        {box.title}
      </p>
      <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {box.description}
      </p>
      <AnchorChips box={box} />
      {visibleQuote && (
        <p
          className="ap-register-evidence ap-soft mt-1.5 truncate"
          style={{ fontSize: TYPE.scale.xs }}
          data-testid="proposal-anchor-quote"
        >
          &ldquo;{visibleQuote}&rdquo;
        </p>
      )}
      {box.sources_outside_view > 0 && (
        <p className="ap-soft mt-1.5" style={{ fontSize: TYPE.scale.xs, fontStyle: "italic" }}>
          {box.sources_outside_view} of {box.sources_total}{" "}
          {box.sources_outside_view === 1
            ? "source is outside your view and stays hidden."
            : "sources are outside your view and stay hidden."}
        </p>
      )}
    </li>
  );
}

function ProposalCard({
  proposal,
  actor,
  busy,
  feedback,
  onDecide,
}: {
  proposal: WorkflowProposal;
  actor: string;
  busy: boolean;
  feedback: { kind: "success" | "error"; text: string } | null;
  onDecide: (decision: "approve" | "deny") => void;
}) {
  const isApprover = proposal.approver_id === actor;
  const canDecide = isApprover && proposal.status === "pending";
  return (
    <article
      className="ap-elevated rounded-2xl border p-4"
      data-testid="proposal-card"
      data-proposal-id={proposal.proposal_id}
      data-can-decide={canDecide ? "true" : "false"}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            {proposal.proposal_id}
          </p>
          <h3 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 600 }}>
            {proposal.title}
          </h3>
        </div>
        <span
          className="ap-chip shrink-0 rounded-full px-2 py-0.5 uppercase tracking-wide"
          data-testid="proposal-status-chip"
          style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
        >
          {statusChip(proposal)}
        </span>
      </div>

      <div className="mt-2 flex items-center gap-2">
        <PersonAvatar principalId={proposal.proposer_id} size={24} />
        <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          <span className="ap-register-evidence">{proposal.proposer_id}</span> proposed ·{" "}
          <span className="ap-register-evidence">{proposal.approver_id}</span> decides
        </p>
      </div>

      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        {proposal.drafted_from}
      </p>

      <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs }} data-testid="proposal-grounding-line">
        {proposal.grounding.admitted} {proposal.grounding.admitted === 1 ? "step" : "steps"}{" "}
        grounded verbatim
        {proposal.grounding.refused > 0 && (
          <span data-testid="proposal-refused-line">
            {" "}
            · {proposal.grounding.refused} draft {proposal.grounding.refused === 1 ? "step" : "steps"}{" "}
            {REFUSAL_NOTE}
          </span>
        )}
        .
      </p>

      <ul className="mt-3 space-y-2">
        {proposal.boxes.map((box) => (
          <ProposalBox key={box.box_index} box={box} />
        ))}
      </ul>

      {canDecide ? (
        <div className="mt-3">
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => onDecide("approve")}
              data-testid="proposal-gate-approve"
              aria-label={`Approve the proposal ${proposal.title}`}
              className="ap-affordance-button ap-register-chrome flex-1 rounded-lg px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            >
              Approve — make it real work
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => onDecide("deny")}
              data-testid="proposal-gate-deny"
              aria-label={`Deny the proposal ${proposal.title}`}
              className="ap-washable ap-register-chrome flex-1 rounded-lg border px-3 py-2"
              style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
            >
              Deny
            </button>
          </div>
          <p className="ap-soft mt-2" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
            Your decision is recorded before anything changes. Approval materializes these steps
            into the pipeline; denial writes nothing.
          </p>
        </div>
      ) : (
        proposal.status === "pending" && (
          <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs }} data-testid="proposal-gate-note">
            Awaiting <span className="ap-register-evidence">{proposal.approver_id}</span> — a
            proposal becomes real work only when its approver decides.
          </p>
        )
      )}

      {feedback && (
        <p
          role="status"
          aria-live="polite"
          data-testid="proposal-gate-feedback"
          className="ap-soft mt-2"
          style={{ fontSize: TYPE.scale.xs }}
        >
          {feedback.text}
        </p>
      )}
    </article>
  );
}

export function ProposalsPanel({
  actor,
  capabilityId,
  onMaterialized,
}: {
  actor: string;
  capabilityId: string;
  onMaterialized: () => void | Promise<void>;
}) {
  const [proposals, setProposals] = useState<WorkflowProposal[] | null>(null);
  const [available, setAvailable] = useState(true);
  const [loading, setLoading] = useState(true);
  const [title, setTitle] = useState("");
  const [goal, setGoal] = useState("");
  const [createBusy, setCreateBusy] = useState(false);
  const [createNote, setCreateNote] = useState<string | null>(null);
  const [decideBusyId, setDecideBusyId] = useState<string | null>(null);
  const [decideFeedback, setDecideFeedback] = useState<{
    id: string;
    kind: "success" | "error";
    text: string;
  } | null>(null);

  const reload = useCallback(async () => {
    try {
      const [mine, inbox] = await Promise.all([
        api.getWorkflowProposals(actor, "proposer"),
        api.getWorkflowProposals(actor, "approver"),
      ]);
      const seen = new Set<string>();
      const merged = [...inbox.proposals, ...mine.proposals].filter((p) => {
        if (p.capability_id !== capabilityId || seen.has(p.proposal_id)) return false;
        seen.add(p.proposal_id);
        return true;
      });
      merged.sort((a, b) => b.created_ordinal - a.created_ordinal);
      setProposals(merged);
      setAvailable(true);
    } catch {
      setProposals(null);
      setAvailable(false);
    } finally {
      setLoading(false);
    }
  }, [actor, capabilityId]);

  useEffect(() => {
    setLoading(true);
    void reload();
  }, [reload]);

  const submit = async () => {
    if (title.trim() === "" || goal.trim() === "" || createBusy) return;
    setCreateBusy(true);
    setCreateNote(null);
    try {
      const result = await api.postWorkflowProposal(actor, {
        capability_id: capabilityId,
        title: title.trim(),
        goal: goal.trim(),
      });
      if (api.proposalWasDrafted(result)) {
        setTitle("");
        setGoal("");
        setCreateNote(
          `Drafted ${result.proposal.boxes.length} grounded ${
            result.proposal.boxes.length === 1 ? "step" : "steps"
          } — awaiting ${result.proposal.approver_id}.`,
        );
        await reload();
      } else {
        const empty = result as WorkflowProposalEmpty;
        const refused = empty.grounding?.refused ?? 0;
        setCreateNote(
          refused > 0
            ? `${empty.reason} ${refused} draft ${refused === 1 ? "step" : "steps"} ${REFUSAL_NOTE}. Nothing was written.`
            : `${empty.reason} Nothing was written.`,
        );
      }
    } catch (error) {
      if (isRequestError(error, "service") && error.error.kind === "service" && error.error.status === 429) {
        setCreateNote("Generation limit reached for now — try again in a minute.");
      } else if (isRequestError(error, "timeout")) {
        setCreateNote("Drafting took too long and was stopped. Nothing was written.");
      } else {
        setCreateNote("Drafting is not available right now. Nothing was written.");
      }
    } finally {
      setCreateBusy(false);
    }
  };

  const decide = async (proposal: WorkflowProposal, decision: "approve" | "deny") => {
    setDecideBusyId(proposal.proposal_id);
    setDecideFeedback(null);
    try {
      await api.postWorkflowProposalDecision(actor, proposal.proposal_id, decision);
      setDecideFeedback({
        id: proposal.proposal_id,
        kind: "success",
        text:
          decision === "approve"
            ? "Approved. The steps are now real pipeline work."
            : "Denied. Nothing was materialized.",
      });
      await reload();
      if (decision === "approve") await onMaterialized();
    } catch {
      setDecideFeedback({
        id: proposal.proposal_id,
        kind: "error",
        text: "The decision was not recorded. Refresh and try again.",
      });
    } finally {
      setDecideBusyId(null);
    }
  };

  return (
    <MotionSection
      aria-labelledby="proposals-heading"
      className="mt-4"
      data-testid="proposals-panel"
    >
      <h2
        id="proposals-heading"
        className="ap-register-chrome"
        style={{ fontSize: TYPE.scale.md, fontWeight: 600, lineHeight: TYPE.line.display }}
      >
        Grounded proposals
      </h2>
      <p className="ap-soft mt-1 max-w-2xl" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
        A model drafts a staged plan from documents you can already see — every step anchored to a
        verbatim quote. Nothing becomes real work until the accountable approver decides.
      </p>

      {available && (
        <form
          className="ap-card mt-3 rounded-2xl border p-4"
          data-testid="proposal-create"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
            Draft a grounded plan
          </p>
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
            <label className="block">
              <span className="ap-soft block" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                Plan title
              </span>
              <input
                type="text"
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                className="mt-1 w-full rounded-lg px-3 py-2"
                style={{ fontSize: TYPE.scale.sm }}
                data-testid="proposal-create-title"
                maxLength={120}
              />
            </label>
            <label className="block">
              <span className="ap-soft block" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
                What should the plan draw from?
              </span>
              <input
                type="text"
                value={goal}
                onChange={(event) => setGoal(event.target.value)}
                className="mt-1 w-full rounded-lg px-3 py-2"
                style={{ fontSize: TYPE.scale.sm }}
                data-testid="proposal-create-goal"
                maxLength={200}
              />
            </label>
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <button
              type="submit"
              disabled={createBusy || title.trim() === "" || goal.trim() === ""}
              className="ap-affordance-button ap-register-chrome rounded-lg px-4 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="proposal-create-submit"
            >
              {createBusy ? "Drafting from your sources…" : "Draft from my sources"}
            </button>
            <p
              role="status"
              aria-live="polite"
              className="ap-soft min-w-0"
              style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}
              data-testid="proposal-create-status"
            >
              {createNote ?? ""}
            </p>
          </div>
        </form>
      )}

      {loading && (
        <div className="ap-card mt-3 rounded-lg p-4" data-testid="proposals-loading">
          <Skeleton lines={3} />
        </div>
      )}

      {!loading && !available && (
        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs }} data-testid="proposals-unavailable">
          Proposals are not available right now.
        </p>
      )}

      {!loading && available && proposals !== null && proposals.length === 0 && (
        <p className="ap-soft mt-3" style={{ fontSize: TYPE.scale.xs }} data-testid="proposals-empty">
          No proposals yet for this project.
        </p>
      )}

      {!loading && available && proposals !== null && proposals.length > 0 && (
        <div className="mt-3 space-y-3">
          {proposals.map((proposal) => (
            <ProposalCard
              key={proposal.proposal_id}
              proposal={proposal}
              actor={actor}
              busy={decideBusyId === proposal.proposal_id}
              feedback={decideFeedback?.id === proposal.proposal_id ? decideFeedback : null}
              onDecide={(decision) => decide(proposal, decision)}
            />
          ))}
        </div>
      )}
    </MotionSection>
  );
}
