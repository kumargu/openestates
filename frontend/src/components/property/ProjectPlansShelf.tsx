import { useMemo, useState } from "react";
import type { ProjectPlansView } from "../../lib/types.ts";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";

type Props = {
  propertyId: string;
  plans: ProjectPlansView;
};

function formatSqft(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return null;
  return `${value.toLocaleString("en-IN")} sqft`;
}

export function hasProjectPlans(plans: ProjectPlansView | null | undefined): boolean {
  if (!plans) return false;
  return Boolean(plans.site_overview?.preview_url) || plans.floor_plans.length > 0;
}

export function ProjectPlansShelf({ propertyId, plans }: Props) {
  const floorPlans = plans.floor_plans;
  const [activeId, setActiveId] = useState(floorPlans[0]?.id ?? "");
  const active = useMemo(
    () => floorPlans.find((plan) => plan.id === activeId) ?? floorPlans[0],
    [activeId, floorPlans],
  );

  if (!hasProjectPlans(plans)) return null;

  const carpet = active ? formatSqft(active.carpet_area_sqft) : null;
  const sale = active ? formatSqft(active.sale_area_sqft) : null;
  const sourceUrl = active?.source_url || plans.source_url || plans.site_overview?.source_url;

  return (
    <section className="project-plans" aria-label="Floor plans">
      <div className="project-plans__header">
        <div>
          <p className="project-plans__kicker">Floor plans</p>
          <h2>Plans</h2>
        </div>
      </div>

      {plans.site_overview?.preview_url && (
        <figure className="project-plans__site">
          <div className="project-plans__image-anchor notebook-comment-surface">
            <img
              src={plans.site_overview.preview_url}
              alt={plans.site_overview.label || "Site overview"}
            />
            <NotebookCommentAnchor
              propertyId={propertyId}
              labels={["layout"]}
              detail={plans.site_overview.label || "Site overview"}
              source="Plans"
            />
          </div>
          <figcaption>
            <span>{plans.site_overview.label || "Site overview"}</span>
            {plans.site_overview.source_url && (
              <a href={plans.site_overview.source_url} target="_blank" rel="noreferrer">
                Source
              </a>
            )}
          </figcaption>
        </figure>
      )}

      {floorPlans.length > 0 && active && (
        <>
          <div className="project-plans__tabs" role="tablist" aria-label="Bedroom types">
            {floorPlans.map((plan) => (
              <button
                key={plan.id}
                type="button"
                role="tab"
                aria-selected={plan.id === active.id}
                className={plan.id === active.id ? "is-active" : undefined}
                onClick={() => setActiveId(plan.id)}
              >
                {plan.tab_label}
              </button>
            ))}
          </div>

          <div className="project-plans__body">
            <div className="project-plans__visual notebook-comment-surface">
              <img src={active.preview_url} alt={active.title} />
              <NotebookCommentAnchor
                propertyId={propertyId}
                labels={["layout"]}
                detail={active.title}
                source="Floor plan"
              />
            </div>
            <div className="project-plans__meta">
              <h3>{active.title}</h3>
              <dl>
                {carpet && (
                  <div>
                    <dt>Carpet</dt>
                    <dd>{carpet}</dd>
                  </div>
                )}
                {sale && (
                  <div>
                    <dt>Sale</dt>
                    <dd>{sale}</dd>
                  </div>
                )}
                {typeof active.usable_area_ratio === "number" && (
                  <div>
                    <dt>Usable</dt>
                    <dd>{Math.round(active.usable_area_ratio * 100)}% carpet</dd>
                  </div>
                )}
              </dl>
              {sourceUrl && (
                <a className="project-plans__source" href={sourceUrl} target="_blank" rel="noreferrer">
                  Source
                </a>
              )}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
