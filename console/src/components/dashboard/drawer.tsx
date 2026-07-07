"use client";
import type { ProjectRecord } from "@/lib/api";
import { useReducedMotion } from "framer-motion";
import { MotionAnchor } from "../MotionPrimitives";
import { ThemeToggle } from "../ThemeToggle";
import { EmptyLine, isDepartmentHead, scopeModeLabel } from "./shared";
import type { NotificationItem } from "./shared";

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "framer-motion";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  GraphResponse,
  LensResponse,
  NodeSummary,
  RoleScopeSummary,
  WorkflowItem,
} from "@/lib/api";
import { RADIUS, TYPE } from "@/lib/tokens";
import { useModalDialogFocus } from "../A11yDialog";
import { MotionSection } from "../MotionPrimitives";
import { PersonAvatar } from "../PersonAvatar";
import {
  Chip,
  Panel,
  WorkspaceBlock,
  WorkspaceFact,
  dashboardPanelStyle,
  type DashboardPanelMode,
} from "./shared";
import { deriveConnectedSystems } from "./model";
import { NotificationCenter, RoleAwareWorkflowLayer, RoleExperienceSummary, WorkflowCommandSubbar, WorkspaceNotifications } from "./notifications";
import { AgentsList, GrantedKnowledgeList, KnowledgeSummary, ProjectsList, RequestsList, ScopePosture, WorkflowSummary } from "./lists";

export function DashboardPanelTabs({
  active,
  onSelect,
}: {
  active: DashboardPanelMode | null;
  onSelect: (mode: DashboardPanelMode) => void;
}) {
  // Profile opens from the identity strip (avatar + name); only Workspace and
  // Settings remain as header pill triggers.
  const tabs: { label: string; mode: DashboardPanelMode }[] = [
    { label: "Workspace", mode: "workspace" },
    { label: "Settings", mode: "settings" },
  ];

  return (
    <div
      className="ap-card flex flex-wrap gap-1 rounded-full p-1"
      data-testid="dashboard-panel-tabs"
      aria-label="Open cockpit panels"
    >
      {tabs.map((tab) => {
        const selected = active === tab.mode;
        return (
          <button
            key={tab.mode}
            type="button"
            className={`${selected ? "ap-affordance-button" : "ap-washable"} ap-register-chrome inline-flex min-h-10 items-center gap-1.5 rounded-full px-3 py-2`}
            style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
            onClick={() => onSelect(tab.mode)}
            aria-haspopup="dialog"
            aria-expanded={selected}
            data-active={selected ? "true" : "false"}
            data-testid={`dashboard-${tab.mode}-panel-trigger`}
          >
            {/* Side-panel glyph: marks this as a control that OPENS a panel,
                not a route tab that swaps content in place (no-affordance contract). */}
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
              <rect x="2.5" y="3" width="11" height="10" rx={RADIUS.glyph} />
              <line x1="10" y1="3" x2="10" y2="13" />
            </svg>
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}


export function DashboardPanelDrawer({
  children,
  mode,
  onClose,
}: {
  children: React.ReactNode;
  mode: DashboardPanelMode | null;
  onClose: () => void;
}) {
  const shouldReduce = useReducedMotion() ?? false;
  const title = mode === "workspace" ? "Workspace" : mode === "profile" ? "Profile" : "Settings";
  // B6: the drawer's focus management (focus-in on open, Tab trap, Escape
  // close, focus-restore on close) now comes from the SHARED primitive this
  // pattern was extracted into — one implementation for every drawer.
  const { dialogRef: asideRef, onKeyDown } = useModalDialogFocus({
    open: mode !== null,
    onClose,
  });

  return (
    <AnimatePresence>
      {mode !== null && (
        <>
          <motion.button
            type="button"
            className="ap-glass-scrim fixed inset-0 z-40 cursor-default"
            aria-label={`Close ${title}`}
            tabIndex={-1}
            data-testid="dashboard-drawer-scrim"
            onClick={onClose}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: shouldReduce ? 0 : 0.18 }}
          />
          <motion.aside
            ref={asideRef}
            role="dialog"
            aria-modal="true"
            aria-label={`${title} panel`}
            tabIndex={-1}
            onKeyDown={onKeyDown}
            className="ap-glass-popover fixed bottom-3 right-3 top-3 z-50 w-[min(456px,calc(100vw-24px))] overflow-y-auto rounded-2xl p-3"
            data-testid="dashboard-active-drawer"
            initial={{ opacity: 0, x: shouldReduce ? 0 : 42 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: shouldReduce ? 0 : 28 }}
            transition={{ duration: shouldReduce ? 0 : 0.18, ease: [0.16, 1, 0.3, 1] }}
          >
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
                {title}
              </p>
              <button
                type="button"
                className="ap-washable ap-register-chrome min-h-10 rounded-full border px-3 py-2"
                style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 700 }}
                onClick={onClose}
                data-testid="dashboard-drawer-close"
              >
                Close
              </button>
            </div>
            {children}
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}


export function WorkspacePanel({
  actor,
  grantError,
  grants,
  inbox,
  notificationItems,
  onRevokeGrant,
  projectById,
  projects,
  requests,
  revokingGrantId,
  roleScope,
  workflowItems,
}: {
  actor: string;
  grantError: string | null;
  grants: AccessGrantRecord[];
  inbox: AccessRequestRecord[];
  notificationItems: NotificationItem[];
  onRevokeGrant: (grantId: string) => void;
  projectById: Map<string, GraphProject>;
  projects: ProjectRecord[];
  requests: AccessRequestRecord[];
  revokingGrantId: string | null;
  roleScope: RoleScopeSummary | null;
  workflowItems: WorkflowItem[];
}) {
  const waitingWorkflowItems = workflowItems.filter((item) =>
    ["pending", "blocked", "denied", "cancelled", "expired", "dismissed"].includes(item.status.toLowerCase()),
  );

  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-workspace"
      id="dashboard-workspace"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Workspace
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Work needing a decision
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      <div className="space-y-3">
        <WorkspaceNotifications items={notificationItems} />
        <WorkflowCommandSubbar items={notificationItems} />

        <WorkspaceBlock title="Requests and approvals" defaultOpen>
          <RequestsList
            actor={actor}
            grantError={grantError}
            grants={grants}
            inbox={inbox}
            onRevokeGrant={onRevokeGrant}
            projectById={projectById}
            requests={requests}
            revokingGrantId={revokingGrantId}
          />
        </WorkspaceBlock>

        <WorkspaceBlock title="Granted Knowledge" defaultOpen>
          <GrantedKnowledgeList actor={actor} grants={grants} projectById={projectById} />
        </WorkspaceBlock>

        <WorkspaceBlock title="Workflow alerts">
          {waitingWorkflowItems.length === 0 ? (
            <EmptyLine compact>No waiting or blocked workflow rows are visible.</EmptyLine>
          ) : (
            <div className="space-y-2">
              {waitingWorkflowItems.slice(0, 5).map((item, index) => (
                <MotionAnchor
                  key={item.item_id}
                  href={`/project?cap=${encodeURIComponent(item.capability_id)}&as=${encodeURIComponent(actor)}`}
                  className="ap-card ap-washable block rounded-lg p-2"
                  delayIndex={index}
                  data-testid="dashboard-workspace-workflow-alert"
                >
                  <div className="flex items-start justify-between gap-2">
                    <p className="ap-register-chrome min-w-0" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {item.title}
                    </p>
                    <Chip>{item.status}</Chip>
                  </div>
                  <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                    {item.capability_id}
                  </p>
                </MotionAnchor>
              ))}
            </div>
          )}
        </WorkspaceBlock>

        {(roleScope?.team_scope.has_team_scope || isDepartmentHead(roleScope?.derived_level)) && (
          <WorkspaceBlock title="Team and department context">
            {roleScope?.team_scope.has_team_scope && (
              <WorkspaceFact
                label="Team requests"
                source="reporting line"
                value={`${roleScope.team_scope.direct_report_count} direct ${roleScope.team_scope.direct_report_count === 1 ? "report" : "reports"}`}
              />
            )}
            {isDepartmentHead(roleScope?.derived_level) && roleScope?.department_scope.department_id && (
              <WorkspaceFact
                label="Department context"
                source="server scope"
                value={roleScope.department_scope.department_id}
              />
            )}
            <EmptyLine compact>Team workflow rows appear only when the API exposes them.</EmptyLine>
          </WorkspaceBlock>
        )}

        <WorkspaceBlock title="Visible workflow layers">
          <RoleAwareWorkflowLayer
            actor={actor}
            inbox={inbox}
            projectById={projectById}
            projects={projects}
            requests={requests}
            roleScope={roleScope}
            workflowItems={workflowItems}
          />
        </WorkspaceBlock>
      </div>
    </MotionSection>
  );
}


export function SettingsPanel({
  graph,
  roleScope,
  summary,
}: {
  graph: GraphResponse | null;
  roleScope: RoleScopeSummary | null;
  summary: NodeSummary | null;
}) {
  const systems = deriveConnectedSystems(graph);
  return (
    <MotionSection
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-settings-panel"
      id="dashboard-settings"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Settings
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Display, systems, and preferences
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      <div className="grid grid-cols-1 gap-3">
        <WorkspaceBlock title="Theme" defaultOpen>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                Light or dark mode
              </p>
              <p className="ap-soft mt-1" style={{ fontSize: TYPE.scale.xs, lineHeight: TYPE.line.body }}>
                Choose the display mode for this browser. Dark remains the default for the demo.
              </p>
            </div>
            <ThemeToggle compact />
          </div>
        </WorkspaceBlock>

        <WorkspaceBlock title="Connected Systems">
          <div className="grid grid-cols-1 gap-2" data-testid="dashboard-connected-systems">
            {systems.length === 0 ? (
              <EmptyLine compact>No supported connected systems are visible through this graph.</EmptyLine>
            ) : (
              systems.map((system) => (
                <div
                  key={`${system.name}:${system.source}`}
                  className="ap-card flex items-center justify-between gap-3 rounded-lg p-2"
                  data-testid="dashboard-connected-system"
                >
                  <div className="min-w-0">
                    <p className="ap-register-chrome" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                      {system.name}
                    </p>
                    <p className="ap-register-evidence ap-soft mt-1" style={{ fontSize: TYPE.scale.xs }}>
                      {system.source}
                    </p>
                  </div>
                  <Chip>{system.status}</Chip>
                </div>
              ))
            )}
          </div>
        </WorkspaceBlock>

        <WorkspaceBlock title="Preferences">
          <WorkspaceFact label="Workspace preferences" source="not modeled" value="Unavailable" />
        </WorkspaceBlock>

        <WorkspaceBlock title="Agent Preferences">
          <WorkspaceFact
            label="Owned agents"
            source="node summary"
            value={`${summary?.agents_owned?.length ?? 0} owned agents visible`}
          />
          <WorkspaceFact
            label="Agent behavior"
            source="not connected"
            value="Not in this build."
          />
        </WorkspaceBlock>
      </div>
    </MotionSection>
  );
}

