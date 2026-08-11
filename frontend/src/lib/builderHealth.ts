import type { BuilderPortfolio, BuilderProjectRecord } from "./types.ts";

export type BuilderMilestoneState = "complete" | "current" | "pending";

export type BuilderMilestone = {
  id: "rera" | "build" | "target" | "handover";
  label: string;
  state: BuilderMilestoneState;
};

export type BuilderHealthSummary = {
  label: string;
  read: string;
  tone: "clear" | "watch" | "neutral";
  flaggedProjects: number;
  metrics: {
    projects: number;
    reraLinked: number;
    delayed: number;
    complaints: number;
  };
};

function projectArtifactKey(project: BuilderProjectRecord): string {
  const reraNumber = project.rera_number?.trim().toLowerCase();
  if (reraNumber) return `rera:${reraNumber}`;
  return `project:${project.project_name.trim().toLowerCase()}|${project.area.trim().toLowerCase()}`;
}

function hasProjectArtifact(project: BuilderProjectRecord): boolean {
  return Boolean(
    project.rera_number
    || project.rera_status
    || project.rera_registered
    || project.start_date
    || project.completion_date
    || project.project_status_display
    || (project.delay_months != null && project.delay_months > 0)
    || (project.complaints_count != null && project.complaints_count > 0),
  );
}

export function uniqueBuilderProjects(portfolio: BuilderPortfolio): BuilderProjectRecord[] {
  const projects = new Map<string, BuilderProjectRecord>();
  for (const project of portfolio.projects) {
    const key = projectArtifactKey(project);
    const existing = projects.get(key);
    if (!existing || (!existing.current && project.current)) projects.set(key, project);
  }
  return [...projects.values()];
}

export function hasRelatedBuilderEvidence(portfolio?: BuilderPortfolio | null): portfolio is BuilderPortfolio {
  if (!portfolio) return false;
  return uniqueBuilderProjects(portfolio).some(
    (project) => !project.current && hasProjectArtifact(project),
  );
}

function normalizedProjectState(project: BuilderProjectRecord): string {
  return (project.project_status_display ?? "")
    .split("·", 1)[0]
    .trim()
    .toLowerCase()
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ");
}

function isDeliveredStatus(project: BuilderProjectRecord): boolean {
  return new Set([
    "delivered",
    "completed",
    "ready to move",
    "occupancy certificate issued",
  ]).has(normalizedProjectState(project));
}

function isActiveBuildStatus(project: BuilderProjectRecord): boolean {
  return new Set([
    "under construction",
    "construction in progress",
    "ongoing",
  ]).has(normalizedProjectState(project));
}

export function builderProjectMilestones(project: BuilderProjectRecord): BuilderMilestone[] {
  const delivered = isDeliveredStatus(project);
  const activeBuild = isActiveBuildStatus(project);

  return [
    {
      id: "rera",
      label: "RERA",
      state: project.rera_registered ? "complete" : "pending",
    },
    {
      id: "build",
      label: "Build",
      state: delivered ? "complete" : activeBuild ? "current" : "pending",
    },
    {
      id: "target",
      label: "Target",
      state: delivered ? "complete" : "pending",
    },
    { id: "handover", label: "Handover", state: delivered ? "complete" : "pending" },
  ];
}

export function builderHealthSummary(portfolio: BuilderPortfolio): BuilderHealthSummary {
  const projects = uniqueBuilderProjects(portfolio);
  const reraLinked = projects.filter(
    (project) => project.rera_registered,
  ).length;
  const delayed = projects.filter(
    (project) => project.delay_months != null && project.delay_months > 0,
  ).length;
  const complaints = projects.filter(
    (project) => project.complaints_count != null && project.complaints_count > 0,
  ).length;
  const flaggedProjects = projects.filter(
    (project) =>
      (project.delay_months != null && project.delay_months > 0)
      || (project.complaints_count != null && project.complaints_count > 0),
  ).length;
  const label = "Regulatory history";
  const tone = "neutral";
  const parts = [
    `${projects.length} related project${projects.length === 1 ? "" : "s"}`,
    `${reraLinked} RERA-linked`,
  ];
  if (delayed > 0) parts.push(`${delayed} delayed`);
  if (complaints > 0) {
    parts.push(`${complaints} with complaints`);
  }
  return {
    label,
    read: parts.join(" · "),
    tone,
    flaggedProjects,
    metrics: {
      projects: projects.length,
      reraLinked,
      delayed,
      complaints,
    },
  };
}
