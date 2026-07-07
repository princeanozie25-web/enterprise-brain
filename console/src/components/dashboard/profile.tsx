"use client";
import { Chip, EmptyLine, WorkspaceBlock, WorkspaceFact, roleLabel, scopeModeLabel } from "./shared";

import type {
  AccessGrantRecord,
  AccessRequestRecord,
  GraphProject,
  LensResponse,
  NodeSummary,
  RoleScopeSummary,
} from "@/lib/api";
import { TYPE } from "@/lib/tokens";
import { MotionSection } from "../MotionPrimitives";
import {
  Panel,
  type RoleExperienceCard,
  type ScopeBadge,
} from "./shared";
import { RoleExperienceSummary } from "./notifications";
import { AgentsList, GrantedKnowledgeList, KnowledgeSummary, ProjectsList, RequestsList, ScopePosture, WorkflowSummary } from "./lists";

export function ProfilePanel({
  actor,
  grants,
  human,
  inbox,
  lens,
  projectById,
  requests,
  roleExperienceCards,
  roleScope,
  scopeBadges,
  summary,
}: {
  actor: string;
  grants: AccessGrantRecord[];
  human: LensResponse["subject_human"];
  inbox: AccessRequestRecord[];
  lens: LensResponse;
  projectById: Map<string, GraphProject>;
  requests: AccessRequestRecord[];
  roleExperienceCards: RoleExperienceCard[];
  roleScope: RoleScopeSummary | null;
  scopeBadges: ScopeBadge[];
  summary: NodeSummary | null;
}) {
  const directReports = roleScope?.team_scope.direct_report_count ?? human?.manages.length ?? 0;
  const knowledgeSections = lens.holdings.length;
  const visibleKnowledgeRows = lens.holdings.reduce((sum, section) => sum + section.docs.length, 0);
  const auditRows = [
    ...requests.map((request) => ({
      id: request.request_id,
      label: "Request",
      status: request.status,
      target: request.target.capability_id,
    })),
    ...inbox.map((request) => ({
      id: request.request_id,
      label: "Approval",
      status: request.status,
      target: request.target.capability_id,
    })),
    ...grants.map((grant) => ({
      id: grant.grant_id,
      label: "Grant",
      status: grant.status,
      target: grant.target.capability_id,
    })),
  ];

  return (
    <section
      className="ap-card rounded-2xl p-3"
      data-testid="dashboard-profile-panel"
      id="dashboard-profile"
    >
      <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="ap-register-evidence ap-soft" style={{ fontSize: TYPE.scale.xs }}>
            Profile
          </p>
          <h2 className="ap-register-chrome mt-1" style={{ fontSize: TYPE.scale.md, fontWeight: 700 }}>
            Identity, access, and knowledge
          </h2>
        </div>
        <Chip>{scopeModeLabel(roleScope)}</Chip>
      </div>

      {/* A4: the page's single demo-status line is the shell's notice — the
          dashboard no longer stacks its own copy. */}
      <div className="grid grid-cols-1 gap-3">
        <div className="grid grid-cols-1 gap-3">
          <WorkspaceBlock title="Identity" defaultOpen>
            <WorkspaceFact label="Identity ID" source="selected Work Identity" value={actor} />
            <WorkspaceFact label="Name" source="Work Identity" value={human?.display_name ?? lens.subject.name} />
            <WorkspaceFact label="Title" source="people record" value={human?.title ?? "Role unavailable"} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Role" defaultOpen>
            <WorkspaceFact
              label="Role posture"
              source="server scope"
              value={roleScope ? roleLabel(roleScope.derived_level) : "Role posture unavailable"}
            />
            <WorkspaceFact
              label="Confidence"
              source="server scope"
              value={roleScope?.confidence ?? "unavailable"}
            />
            <WorkspaceFact
              label="Surface access"
              source="scope contract"
              value={roleScope?.admin_surface_allowed ? "restricted preview candidate" : "daily work only"}
            />
          </WorkspaceBlock>

          <WorkspaceBlock title="Department">
            <WorkspaceFact
              label="Department"
              source="Work Identity"
              value={human?.department_label ?? lens.subject.department ?? "Department unavailable"}
            />
            <WorkspaceFact label="Manager" source="people record" value={human?.reports_to ?? "Manager unavailable"} />
            <WorkspaceFact
              label="Team"
              source="reporting line"
              value={directReports > 0 ? `${directReports} direct ${directReports === 1 ? "report" : "reports"}` : "No team scope"}
            />
          </WorkspaceBlock>

          <WorkspaceBlock title="Security">
            <WorkspaceFact label="Band" source="scope" value={String(lens.subject.band ?? "unavailable")} />
            <WorkspaceFact label="Groups" source="scope" value={lens.subject.groups.join(", ") || "No groups visible"} />
            <WorkspaceFact label="Sites" source="scope" value={lens.subject.sites.join(", ") || "No sites visible"} />
            <WorkspaceFact label="Restricted surfaces" source="scope contract" value="Unavailable on this dashboard" />
          </WorkspaceBlock>
        </div>

        <div className="space-y-3">
          <WorkspaceBlock title="Audit Activity">
            <div className="space-y-2" data-testid="dashboard-audit-activity">
              {auditRows.length === 0 ? (
                <EmptyLine compact>No request, approval, or grant ledger rows are visible.</EmptyLine>
              ) : (
                auditRows.slice(0, 5).map((row) => {
                  const project = projectById.get(row.target);
                  return (
                    <div key={`${row.label}:${row.id}`} className="ap-card rounded-lg p-2">
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0">
                          <p className="ap-register-chrome truncate" style={{ fontSize: TYPE.scale.sm, fontWeight: 600 }}>
                            {row.label} {row.id}
                          </p>
                          <p className="ap-soft mt-1 truncate" style={{ fontSize: TYPE.scale.xs }}>
                            {project?.label.replace(/^Capability:\s*/i, "") ?? row.target}
                          </p>
                        </div>
                        <Chip>{row.status}</Chip>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </WorkspaceBlock>

          <WorkspaceBlock title="Role Experience">
            <RoleExperienceSummary cards={roleExperienceCards} />
          </WorkspaceBlock>

          <WorkspaceBlock title="Scope Posture">
            <ScopePosture badges={scopeBadges} />
          </WorkspaceBlock>

          <WorkspaceBlock title="My Agents">
            <AgentsList agents={summary?.agents_owned ?? []} />
          </WorkspaceBlock>

          <WorkspaceBlock title="My Knowledge">
            <KnowledgeSummary
              sections={knowledgeSections}
              rows={visibleKnowledgeRows}
              holdings={lens.holdings.map((section) => ({
                count: section.docs.length,
                sentence: section.sentence,
              }))}
            />
          </WorkspaceBlock>
        </div>
      </div>
    </section>
  );
}

