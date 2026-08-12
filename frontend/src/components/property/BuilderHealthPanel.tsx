import { useId, useState } from "react";
import { Link } from "react-router-dom";
import {
  builderHealthSummary,
  builderProjectMilestones,
  hasRelatedBuilderEvidence,
  uniqueBuilderProjects,
} from "../../lib/builderHealth.ts";
import type { BuilderPortfolio, BuilderProjectRecord } from "../../lib/types.ts";

function formatCompletionDate(value?: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-IN", {
    month: "short",
    year: "numeric",
  }).format(date);
}

function projectStatus(project: BuilderProjectRecord): string | null {
  if (project.delay_months != null && project.delay_months > 0) {
    return `${project.delay_months} mo delayed`;
  }
  return project.project_status_display ?? project.rera_status ?? null;
}

function complaintLabel(count?: number): string {
  if (count == null || count === 0) return "None recorded";
  return `${count} complaint${count === 1 ? "" : "s"}`;
}

function BuilderMilestoneRail({ project }: { project: BuilderProjectRecord }) {
  const milestones = builderProjectMilestones(project);
  const target = formatCompletionDate(project.completion_date);
  const status = projectStatus(project);

  return (
    <div
      className={`builder-health__timeline${project.delay_months != null && project.delay_months > 0 ? " is-delayed" : ""}`}
      aria-label={`${project.project_name} timeline${status ? `, ${status}` : ""}`}
    >
      <div className="builder-health__timeline-read">
        <strong>{status ?? "Timeline recorded"}</strong>
        {target && <span>Target {target}</span>}
      </div>
      <div className="builder-health__rail" aria-hidden="true">
        {milestones.map((milestone, index) => (
          <span
            key={milestone.id}
            className={`builder-health__milestone is-${milestone.state}`}
          >
            <i />
            <small>{milestone.label}</small>
            {index < milestones.length - 1 && <b />}
          </span>
        ))}
      </div>
    </div>
  );
}

export function BuilderHealthPanel({
  portfolio,
}: {
  portfolio?: BuilderPortfolio | null;
}) {
  const panelId = useId();
  const [open, setOpen] = useState(false);

  if (!hasRelatedBuilderEvidence(portfolio)) return null;

  const projects = uniqueBuilderProjects(portfolio);
  const summary = builderHealthSummary(portfolio);

  return (
    <section className="builder-health" aria-labelledby={`${panelId}-title`}>
      <button
        type="button"
        className="builder-health__summary"
        aria-expanded={open}
        aria-controls={`${panelId}-panel`}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="builder-health__summary-copy">
          <span className="builder-health__title-row">
            <strong id={`${panelId}-title`}>{portfolio.builder_name}</strong>
            <em className={`is-${summary.tone}`}>
              {summary.label}
            </em>
          </span>
          <span>{summary.read}</span>
        </span>
        <span className="builder-health__toggle" aria-hidden="true">
          {open ? "Hide" : "View"}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </span>
      </button>

      <div
        id={`${panelId}-panel`}
        className="builder-health__panel"
        hidden={!open}
        aria-hidden={!open}
      >
        <div className="builder-health__panel-inner">
          <dl className="builder-health__stats">
            <div>
              <dt>RERA linked</dt>
              <dd>{summary.metrics.reraLinked}/{summary.metrics.projects}</dd>
            </div>
            <div>
              <dt>Delayed</dt>
              <dd>{summary.metrics.delayed}</dd>
            </div>
            <div>
              <dt>With complaints</dt>
              <dd>{summary.metrics.complaints}</dd>
            </div>
          </dl>

          <div className="builder-health__table-wrap">
            <table className="builder-health__table">
              <thead>
                <tr>
                  <th>Project</th>
                  <th>RERA</th>
                  <th>Timeline</th>
                  <th>Complaints</th>
                </tr>
              </thead>
              <tbody>
                {projects.map((project) => (
                  <tr key={`${project.property_id}-${project.rera_number ?? project.project_name}`}>
                    <td>
                      <Link to={`/property/${project.property_id}`}>{project.project_name}</Link>
                      <span>
                        {project.area}
                        {project.current ? " · This home" : ""}
                      </span>
                    </td>
                    <td>
                      {project.rera_portal_url && project.rera_number ? (
                        <a href={project.rera_portal_url} target="_blank" rel="noreferrer">
                          {project.rera_number}
                        </a>
                      ) : (
                        <span>{project.rera_number ?? project.rera_status ?? "—"}</span>
                      )}
                    </td>
                    <td><BuilderMilestoneRail project={project} /></td>
                    <td>{complaintLabel(project.complaints_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </section>
  );
}
