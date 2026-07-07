"use client";

import { useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  GraphResponse,
  LensResponse,
  NodeSummary,
  ProjectWorkflowResponse,
  RoleScopeSummary,
} from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionAnchor, MotionSection } from "./MotionPrimitives";
import { PersonAvatar } from "./PersonAvatar";
import { Skeleton } from "./Skeleton";
import { ThemeToggle } from "./ThemeToggle";
// K3 Track 4: EmployeeDashboard is now a shell that composes the extracted
// dashboard sections (behavior-frozen — the code moved verbatim, the render
// tree is identical). Each section lives in ./dashboard/ (each ≤450 lines).
import { Chip, Panel, type DashboardPanelMode } from "./dashboard/shared";
import {
  buildNotificationItems,
  buildRoleExperienceCards,
  deriveScopeBadges,
} from "./dashboard/model";
import { buildTodayCockpit } from "./dashboard/cockpitModel";
import { AskAgentCard, TodayCockpit } from "./dashboard/cockpit";
import { ProjectsList, WorkflowSummary } from "./dashboard/lists";
import {
  DashboardPanelDrawer,
  DashboardPanelTabs,
  WorkspacePanel,
  SettingsPanel,
} from "./dashboard/drawer";
import { ProfilePanel } from "./dashboard/profile";

export function EmployeeDashboard({ actor }: { actor: string | null }) {
  const [lens, setLens] = useState<LensResponse | null>(null);
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [summary, setSummary] = useState<NodeSummary | null>(null);
  const [roleScope, setRoleScope] = useState<RoleScopeSummary | null>(null);
  const [requests, setRequests] = useState<AccessRequestRecord[]>([]);
  const [grants, setGrants] = useState<AccessGrantRecord[]>([]);
  const [inbox, setInbox] = useState<AccessRequestRecord[]>([]);
  const [workflows, setWorkflows] = useState<ProjectWorkflowResponse[]>([]);
  const [grantError, setGrantError] = useState<string | null>(null);
  const [revokingGrantId, setRevokingGrantId] = useState<string | null>(null);
  const [activePanel, setActivePanel] = useState<DashboardPanelMode | null>(null);
  const [loading, setLoading] = useState(false);
  const [available, setAvailable] = useState(true);

  useEffect(() => {
    if (actor === null) {
      setLens(null);
      setGraph(null);
      setSummary(null);
      setRoleScope(null);
      setRequests([]);
      setGrants([]);
      setInbox([]);
      setWorkflows([]);
      setGrantError(null);
      setRevokingGrantId(null);
      setAvailable(true);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setAvailable(true);

    Promise.all([
      api.getLens(actor, actor),
      api.getGraph(actor),
      api.getNodeSummary(actor, actor),
      api.getRoleScope(actor),
      api.getAccessRequests(actor),
      api.getAccessGrants(actor),
      api.getAccessRequestInbox(actor),
    ])
      .then(async ([
        lensResponse,
        graphResponse,
        summaryResponse,
        roleScopeResponse,
        requestResponse,
        grantResponse,
        inboxResponse,
      ]) => {
        if (cancelled) return;
        setLens(lensResponse);
        setGraph(graphResponse);
        setSummary(summaryResponse);
        setRoleScope(roleScopeResponse);
        setRequests(requestResponse?.requests ?? []);
        setGrants(grantResponse?.grants ?? []);
        setInbox(inboxResponse?.requests ?? []);
        setGrantError(null);
        setAvailable(lensResponse !== null);

        const projects = lensResponse?.subject_human?.projects ?? [];
        const workflowResponses = await Promise.all(
          projects.map((project) => api.getProjectWorkflow(actor, project.capability_id)),
        );
        if (!cancelled) {
          setWorkflows(
            workflowResponses.filter(
              (workflow): workflow is ProjectWorkflowResponse => workflow !== null,
            ),
          );
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLens(null);
          setGraph(null);
          setSummary(null);
          setRoleScope(null);
          setRequests([]);
          setGrants([]);
          setInbox([]);
          setWorkflows([]);
          setGrantError(null);
          setRevokingGrantId(null);
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
  }, [actor]);

  const projectById = useMemo(() => {
    const map = new Map<string, GraphProject>();
    for (const project of graph?.projects ?? []) map.set(project.id, project);
    return map;
  }, [graph]);

  if (actor === null) {
    return (
      <main className="min-w-0 flex-1" data-testid="employee-dashboard-empty">
        <MotionSection className="ap-card rounded-lg p-4">
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Work Identity
          </p>
          <h1 className="ap-register-chrome mt-2" style={{ fontSize: TYPE.scale.lg, fontWeight: 600 }}>
            Choose a Work Identity to begin.
          </h1>
          <p className="ap-soft mt-2 max-w-2xl" style={{ fontSize: TYPE.scale.sm, lineHeight: TYPE.line.body }}>
            No employee is selected yet, so Enterprise Brain has no permission scope for work,
            access requests, Granted Knowledge, or Ask. Selecting a Work Identity shows only the
            data available to that identity.
          </p>
          <div className="mt-4 flex flex-wrap gap-2">
            <MotionAnchor
              href="/me?as=p060"
              className="ap-affordance-button ap-register-chrome rounded-lg px-3 py-2"
              style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="employee-empty-start-link"
            >
              Open demo Work Identity
            </MotionAnchor>
            <MotionAnchor
              href="/ask?as=p060"
              className="ap-washable ap-register-chrome rounded-lg border px-3 py-2"
              style={{ borderColor: "var(--hairline)", fontSize: TYPE.scale.xs, fontWeight: 600 }}
              data-testid="employee-empty-ask-link"
            >
              Open Ask with that identity
            </MotionAnchor>
          </div>
        </MotionSection>
      </main>
    );
  }

  if (loading) {
    return (
      <main className="min-w-0 flex-1" data-testid="employee-dashboard-loading">
        <div className="ap-card rounded-lg p-4">
          <Skeleton lines={8} />
        </div>
      </main>
    );
  }

  if (!available || lens === null) {
    return (
      <p className="ap-soft py-8" style={{ fontSize: TYPE.scale.sm }} data-testid="employee-dashboard-unavailable">
        This Work Identity is not available in the current permission scope.
      </p>
    );
  }

  const human = lens.subject_human;
  const projects = human?.projects ?? [];
  const workflowItems = workflows.flatMap((workflow) => workflow.items);
  const scopeBadges = deriveScopeBadges({
    grants,
    human,
    inbox,
    requests,
    roleScope,
    summary,
    subjectDepartment: lens.subject.department,
  });
  const roleExperienceCards = buildRoleExperienceCards({
    actor,
    grants,
    inbox,
    requests,
    roleScope,
    workflowItems,
  });
  const notificationItems = buildNotificationItems({
    actor,
    grants,
    inbox,
    requests,
    roleScope,
    workflowItems,
  });
  const todayCockpit = buildTodayCockpit({
    actor,
    grants,
    inbox,
    projectById,
    projects,
    requests,
    roleScope,
    workflowItems,
  });

  async function revokeGrant(grantId: string) {
    if (!actor) return;
    const actorId = actor;
    setRevokingGrantId(grantId);
    setGrantError(null);
    try {
      const response = await api.postAccessGrantRevoke(actorId, grantId, "approver_revoked");
      setGrants((current) =>
        current.map((grant) => (grant.grant_id === grantId ? response.grant : grant)),
      );
    } catch {
      setGrantError("Grant revoke failed.");
    } finally {
      setRevokingGrantId(null);
    }
  }

  return (
    <main className="min-w-0 flex-1" data-testid="employee-dashboard">
      <header
        className="ap-nav sticky top-2 z-20 mb-3 rounded-2xl px-3 py-2"
        data-layout="compact-strip"
        data-testid="dashboard-cockpit-header"
      >
        <div className="flex flex-wrap items-center gap-3">
          <div
            role="button"
            tabIndex={0}
            onClick={() => setActivePanel("profile")}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                setActivePanel("profile");
              }
            }}
            aria-label="Open profile"
            data-testid="dashboard-identity-open-profile"
            className="ap-washable inline-flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-2xl px-1.5 py-1 text-left"
          >
            <PersonAvatar
              principalId={actor}
              displayName={human?.display_name ?? lens.subject.name}
              department={human?.department_label ?? lens.subject.department ?? null}
              size={40}
            />
            <div className="min-w-0 flex-1">
              <h1
                className="ap-register-chrome mt-1"
                style={{ fontSize: TYPE.scale.md, fontWeight: 700, lineHeight: TYPE.line.display }}
                data-testid="dashboard-user-name"
              >
                {human?.display_name ?? lens.subject.name}
              </h1>
              <p className="ap-soft truncate" style={{ fontSize: TYPE.scale.xs }}>
                {human?.title ?? "Role unavailable"}
                {human?.department_label ? ` / ${human.department_label}` : ""}
              </p>
            </div>
            {/* Trailing chevron: the no-affordance cue that this opens the Profile drawer. */}
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true" className="ap-soft shrink-0">
              <path d="M6 4l4 4-4 4" />
            </svg>
          </div>
          <Chip mono>{actor}</Chip>
          <Chip>Demo Identity Mode</Chip>
          <div className="flex flex-wrap items-center justify-end gap-2" data-testid="dashboard-identity-strip">
            <DashboardPanelTabs active={activePanel} onSelect={(mode) => setActivePanel((current) => (current === mode ? null : mode))} />
            <a
              href={`/ask?as=${encodeURIComponent(actor)}`}
              className="ap-affordance-button ap-register-chrome min-h-10 rounded-full px-3 py-2"
              style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}
              data-testid="dashboard-ask-link"
            >
              Ask
            </a>
            <ThemeToggle compact />
          </div>
        </div>
      </header>

      <section
        className="space-y-3"
        data-testid="dashboard-compact-cockpit"
      >
        <div className="min-w-0 space-y-3" data-testid="dashboard-main-cockpit">
          <TodayCockpit model={todayCockpit} />
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,0.95fr)_minmax(0,1.15fr)_minmax(280px,0.72fr)]">
            <Panel
              title="My Projects"
              action={
                <a
                  className="ap-register-chrome ap-washable rounded-lg px-2 py-1"
                  href={`/project?as=${encodeURIComponent(actor)}`}
                  style={{ fontSize: TYPE.scale.xs, fontWeight: 600 }}
                >
                  Open board / {projects.length}
                </a>
              }
            >
              <ProjectsList actor={actor} projects={projects} projectById={projectById} />
            </Panel>

            <Panel
              title="My Workflow"
              action={<span className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>{workflowItems.length}</span>}
            >
              <WorkflowSummary actor={actor} items={workflowItems} />
            </Panel>
            <AskAgentCard actor={actor} grants={grants} />
          </div>
        </div>
      </section>

      <DashboardPanelDrawer mode={activePanel} onClose={() => setActivePanel(null)}>
        {activePanel === "workspace" ? (
            <WorkspacePanel
              actor={actor}
              grantError={grantError}
              grants={grants}
              inbox={inbox}
              notificationItems={notificationItems}
              onRevokeGrant={revokeGrant}
              projectById={projectById}
              projects={projects}
              requests={requests}
              revokingGrantId={revokingGrantId}
              roleScope={roleScope}
              workflowItems={workflowItems}
            />
          ) : activePanel === "profile" ? (
            <ProfilePanel
              actor={actor}
              grants={grants}
              human={human}
              inbox={inbox}
              lens={lens}
              projectById={projectById}
              requests={requests}
              roleExperienceCards={roleExperienceCards}
              roleScope={roleScope}
              scopeBadges={scopeBadges}
              summary={summary}
            />
          ) : activePanel === "settings" ? (
            <SettingsPanel graph={graph} roleScope={roleScope} summary={summary} />
          ) : null}
      </DashboardPanelDrawer>
    </main>
  );
}

