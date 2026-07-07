"use client";
import { MotionSection } from "../MotionPrimitives";

import type { ReactNode } from "react";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  GraphResponse,
  NodeSummary,
  ProjectRecord,
  RoleScopeSummary,
  WorkflowItem,
} from "@/lib/api";
import { TYPE } from "@/lib/tokens";

export interface ScopeBadge {
  detail: string;
  label: string;
  source: string;
}

export interface NotificationItem {
  category:
    | "Requests"
    | "Approvals"
    | "Workflow Alerts"
    | "Grant Expiry"
    | "Grant Events"
    | "Team Updates"
    | "Department Updates";
  detail: string;
  href: string;
  metric?: string;
  source: string;
  title: string;
}

export interface ConnectedSystem {
  name: string;
  source: string;
  status: "Available";
}


export type DashboardPanelMode = "workspace" | "profile" | "settings";


export const WORKFLOW_GROUPS = [
  { label: "In Progress", statuses: ["active"] },
  { label: "Next", statuses: ["candidate", "planned"] },
  { label: "Waiting", statuses: ["pending"] },
  { label: "Blocked", statuses: ["blocked", "denied", "cancelled", "expired", "dismissed"] },
  { label: "Done", statuses: ["done", "approved"] },
];


export const CONNECTOR_NAMES = ["Gmail", "Outlook", "Teams", "Slack", "Jira", "GitHub", "SharePoint"];


export const TODAY_WORKFLOW_STATUSES = new Set(["pending", "blocked"]);


export type CockpitAction = "Review" | "Open" | "Ask" | "Request" | "Approve" | "Continue";


export interface CockpitItem {
  action: CockpitAction;
  detail: string;
  href: string;
  metric: string;
  source: string;
  title: string;
  tone: "attention" | "steady" | "ask" | "waiting" | "manager";
}


export interface TodayCockpitModel {
  askWithContext: CockpitItem[];
  continueWork: CockpitItem[];
  managerRows: CockpitItem[];
  needsAttention: CockpitItem[];
  waitingOn: CockpitItem[];
}


export interface CommandPodModel {
  callToAction?: string;
  detail: string;
  href: string;
  kind:
    | "work"
    | "project"
    | "team"
    | "department"
    | "approval"
    | "request"
    | "grant"
    | "agent"
    | "ask"
    | "executive";
  metric: string;
  title: string;
}


export interface RoleExperienceCard {
  detail: string;
  label: string;
  metric: string;
  source: string;
  tone: "default" | "active" | "candidate" | "limited";
}


export function workflowGroup(status: string): string {
  for (const group of WORKFLOW_GROUPS) {
    if (group.statuses.includes(status)) return group.label;
  }
  return "Next";
}


export function workflowStatusLabel(status: string): string {
  switch (status.toLowerCase()) {
    case "active":
      return "In progress";
    case "pending":
      return "Waiting";
    case "blocked":
      return "Blocked";
    case "denied":
      return "Denied";
    case "cancelled":
      return "Cancelled";
    case "expired":
      return "Expired";
    case "dismissed":
      return "Dismissed";
    case "done":
      return "Done";
    case "approved":
      return "Approved";
    case "planned":
      return "Planned";
    case "candidate":
    default:
      return "Next";
  }
}

// B2: dashboard panels are SOLID elevation — glass belongs to overlays only.

export function dashboardPanelStyle(): React.CSSProperties {
  return {
    background: "var(--surface-1)",
    boxShadow: "var(--shadow-1)",
  };
}


export function roleLabel(level: RoleScopeSummary["derived_level"]): string {
  switch (level) {
    case "department_head":
      return "Department head signal";
    case "executive_candidate":
      return "Executive candidate signal";
    case "super_admin_candidate":
      return "Restricted-surface candidate signal";
    case "team_lead":
      return "Team lead signal";
    case "employee":
    default:
      return "Employee view";
  }
}


export function isExecutiveCandidate(level: RoleScopeSummary["derived_level"] | null | undefined): boolean {
  return level === "executive_candidate" || level === "super_admin_candidate";
}


export function isDepartmentHead(level: RoleScopeSummary["derived_level"] | null | undefined): boolean {
  return level === "department_head";
}


export function scopeModeLabel(roleScope: RoleScopeSummary | null | undefined): string {
  return roleScope ? "permission preview" : "scope unavailable";
}


export function activeKnowledgeGrants(grants: AccessGrantRecord[], actor: string): AccessGrantRecord[] {
  return grants.filter((grant) => grant.status === "active" && grant.grantee_id === actor);
}


export function plural(count: number, singular: string, pluralLabel = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : pluralLabel}`;
}


export function capabilityTitle(capabilityId: string, projectById: Map<string, GraphProject>): string {
  return projectById.get(capabilityId)?.label.replace(/^Capability:\s*/i, "") ?? capabilityId;
}


export function projectHref(actor: string, capabilityId: string): string {
  return `/project?cap=${encodeURIComponent(capabilityId)}&as=${encodeURIComponent(actor)}`;
}


export function askGrantHref(actor: string, grant: AccessGrantRecord): string {
  return `/ask?as=${encodeURIComponent(actor)}&grant=${encodeURIComponent(
    grant.grant_id,
  )}&cap=${encodeURIComponent(grant.target.capability_id)}`;
}


export function Chip({ children, mono = false }: { children: React.ReactNode; mono?: boolean }) {
  return (
    <span
      className={`ap-chip ${mono ? "ap-register-evidence" : "ap-register-chrome"} rounded-lg px-1.5 py-0.5`}
      style={{ fontSize: TYPE.scale.xs }}
    >
      {children}
    </span>
  );
}


export function Panel({
  action,
  children,
  delayIndex = 0,
  title,
}: {
  action?: React.ReactNode;
  children: React.ReactNode;
  delayIndex?: number;
  title: string;
}) {
  return (
    <MotionSection className="ap-card rounded-2xl p-4" delayIndex={delayIndex} style={dashboardPanelStyle()}>
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
          {title}
        </h2>
        {action}
      </div>
      {children}
    </MotionSection>
  );
}


export function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="ap-card rounded-lg p-2">
      <p className="ap-register-evidence" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
        {value}
      </p>
      <p className="ap-soft" style={{ fontSize: TYPE.scale.xs }}>
        {label}
      </p>
    </div>
  );
}


export function EmptyLine({
  children,
  compact = false,
}: {
  children: React.ReactNode;
  compact?: boolean;
}) {
  return (
    <p
      className="ap-soft"
      style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body, padding: compact ? 0 : 8 }}
      data-testid="dashboard-empty-line"
    >
      {children}
    </p>
  );
}


export function WorkspaceFact({ label, source, value }: { label: string; source: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}>
          {label}
        </p>
        <p className="ap-soft mt-1 break-words" style={{ fontSize: TYPE.scale.xs }}>
          {value}
        </p>
      </div>
      <span className="ap-register-evidence ap-soft shrink-0" style={{ fontSize: TYPE.scale.xs }}>
        {source}
      </span>
    </div>
  );
}


export function WorkspaceBlock({
  children,
  defaultOpen = false,
  title,
}: {
  children: React.ReactNode;
  defaultOpen?: boolean;
  title: string;
}) {
  return (
    <details className="ap-card rounded-lg border p-2.5" open={defaultOpen ? true : undefined}>
      <summary className="ap-washable flex cursor-pointer list-none items-center justify-between gap-3 rounded-lg px-1 py-0.5">
        <h3 className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
          {title}
        </h3>
        <span aria-hidden="true" className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
          Details
        </span>
      </summary>
      <div className="mt-2 space-y-2">{children}</div>
    </details>
  );
}

