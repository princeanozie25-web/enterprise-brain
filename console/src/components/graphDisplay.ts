import type { GraphEdge, GraphResponse } from "@/lib/api";

export type GraphNodeMeta = {
  id: string;
  kind: "organization" | "department" | "person" | "agent" | "system" | "project" | "unknown";
  label: string;
};

export type GraphRelationshipRow = {
  edge: GraphEdge;
  from: GraphNodeMeta;
  key: string;
  relation: string;
  to: GraphNodeMeta;
};

export function graphNodeMeta(graph: GraphResponse, id: string): GraphNodeMeta {
  if (graph.center.id === id) {
    return { id, kind: "organization", label: graph.center.label };
  }

  const department = graph.departments.find((item) => item.id === id);
  if (department) {
    return { id, kind: "department", label: department.label };
  }

  const person = graph.people.find((item) => item.id === id);
  if (person) {
    return { id, kind: "person", label: person.display_name };
  }

  const tool = graph.tools.find((item) => item.id === id);
  if (tool) {
    return { id, kind: tool.kind === "agent" ? "agent" : "system", label: tool.label };
  }

  const source = graph.sources.find((item) => item.id === id);
  if (source) {
    return { id, kind: "system", label: source.label };
  }

  const project = graph.projects.find((item) => item.id === id);
  if (project) {
    return { id, kind: "project", label: project.label.replace(/^Capability:\s*/i, "") };
  }

  return { id, kind: "unknown", label: id };
}

export function graphRelationLabel(kind: GraphEdge["kind"]): string {
  switch (kind) {
    case "reports_to":
      return "reports to";
    case "member_of":
      return "member of";
    case "owns_agent":
      return "owns agent";
    case "system_of":
      return "system of record for";
    case "works_on":
      return "works on";
    case "involves_department":
      return "involves department";
    case "uses":
      return "uses";
  }
}

export function graphRelationshipRows(graph: GraphResponse, nodeId?: string | null): GraphRelationshipRow[] {
  return graph.edges
    .filter((edge) => nodeId == null || edge.from === nodeId || edge.to === nodeId)
    .map((edge, index) => ({
      edge,
      from: graphNodeMeta(graph, edge.from),
      key: `${edge.from}:${edge.kind}:${edge.to}:${index}`,
      relation: graphRelationLabel(edge.kind),
      to: graphNodeMeta(graph, edge.to),
    }));
}
