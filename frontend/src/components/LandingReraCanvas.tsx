import { useEffect, useMemo, useRef, type ReactNode } from "react";
import { useLandingLoopSequence } from "../hooks/useLandingChapterSequence.ts";
import {
  claimValueText,
  displayFactsForSection,
  formatReraDate,
  httpUrl,
  selectReraPlanPreviews,
} from "../lib/reraReportView.ts";
import type {
  PropertyCard,
  PropertyDetailResponse,
  ReraBuyerFact,
  ReraBuyerFactSection,
  ReraEvidenceReportResponse,
  ReraReportSurfaceSection,
} from "../lib/types.ts";

const SECTION_DURATION_MS = 2_800;

type PreviewSectionId = "registration" | "overview" | "schedule" | "quarterly" | "plans" | "complaints" | "builder";

function sectionById(
  sections: ReraBuyerFactSection[] | undefined,
  id: string,
): ReraBuyerFactSection | undefined {
  return sections?.find((section) => section.id === id);
}

function surfaceById(
  sections: ReraReportSurfaceSection[] | undefined,
  id: string,
): ReraReportSurfaceSection | undefined {
  return sections?.find((section) => section.id === id);
}

function humanize(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/(^|\s)\S/g, (letter) => letter.toUpperCase());
}

function buyerFactValue(fact: ReraBuyerFact): string {
  const key = fact.key.toLowerCase();
  return key.endsWith("_date") || key.endsWith("_on")
    ? formatReraDate(fact.value)
    : fact.value;
}

function homeName(property: PropertyCard): string {
  return property.society_name.trim() || property.title;
}

function registrationText(
  detail: PropertyDetailResponse | undefined,
  report: ReraEvidenceReportResponse | undefined,
): string | undefined {
  const official = report?.evidence.claims.find((claim) => claim.predicate === "official_registration_number");
  return official
    ? claimValueText(official.value)
    : report?.evidence.registration_ids[0] ?? detail?.rera?.registration_number ?? undefined;
}

function planPreviewUrl(value?: string): string | null {
  if (value?.startsWith("/media/")) return value;
  return httpUrl(value);
}

function ReportSection({
  id,
  title,
  children,
}: {
  id: PreviewSectionId;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="rera-section" data-landing-rera-section={id}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

export function LandingReraCanvas({
  active,
  detail,
  paused,
  property,
  reducedMotion,
  report,
}: {
  active: boolean;
  detail?: PropertyDetailResponse;
  paused: boolean;
  property: PropertyCard;
  reducedMotion: boolean;
  report?: ReraEvidenceReportResponse;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const buyer = report?.buyer_report;
  const factSections = buyer?.fact_sections;
  const registration = registrationText(detail, report);
  const registrationFacts = sectionById(factSections, "registration")?.facts ?? [];
  const status = registrationFacts.find((fact) => fact.key === "rera_status")?.value ?? detail?.rera?.status;
  const overviewFacts = sectionById(factSections, "overview")?.facts ?? [];
  const overviewSurface = surfaceById(report?.surface.sections, "overview");
  const overviewClaims = report && overviewSurface
    ? displayFactsForSection(overviewSurface, report.evidence)
    : [];
  const knownOverviewValues = new Set(overviewFacts.map((fact) => fact.value.trim().toLowerCase()));
  const extraOverviewClaims = overviewClaims.filter((fact) => !knownOverviewValues.has(fact.value.trim().toLowerCase()));
  const scheduleFacts = sectionById(factSections, "schedule")?.facts ?? [];
  const quarterlySurface = surfaceById(report?.surface.sections, "quarterly_progress");
  const quarterlySeries = report?.evidence.series.find((series) => series.series_type === "quarterly_inventory");
  const quarterlyLabels = Object.fromEntries(
    (quarterlySurface?.selectors ?? []).map((selector) => [selector.key.split(".").at(-1), selector.label]),
  );
  const plansSurface = surfaceById(report?.surface.sections, "plans");
  const planItems = useMemo(() => {
    const plans = detail?.plans;
    if (!plans) return [];
    const filed = selectReraPlanPreviews(
      plans.filed_plan_previews ?? [],
      plansSurface?.preview_kinds ?? [],
    );
    return [
      plans.site_overview && planPreviewUrl(plans.site_overview.preview_url)
        ? {
            id: plans.site_overview.artifact_id,
            label: plans.site_overview.label,
            previewUrl: planPreviewUrl(plans.site_overview.preview_url)!,
            detail: undefined,
          }
        : null,
      ...plans.floor_plans.map((plan) => {
        const previewUrl = planPreviewUrl(plan.preview_url);
        if (!previewUrl) return null;
        return {
          id: plan.artifact_id,
          label: plan.title,
          previewUrl,
          detail: [
            plan.carpet_area_sqft ? `${plan.carpet_area_sqft.toLocaleString("en-IN")} sq ft carpet` : null,
            plan.sale_area_sqft ? `${plan.sale_area_sqft.toLocaleString("en-IN")} sq ft sale area` : null,
          ].filter(Boolean).join(" · ") || undefined,
        };
      }),
      ...filed.map((plan) => {
        const previewUrl = planPreviewUrl(plan.preview_url);
        return previewUrl ? { id: plan.artifact_id, label: plan.label, previewUrl, detail: undefined } : null;
      }),
    ].filter((item): item is { id: string; label: string; previewUrl: string; detail: string | undefined } => item !== null)
      .slice(0, plansSurface?.items_per_page ?? 3);
  }, [detail?.plans, plansSurface]);
  const complaints = buyer?.complaints ?? [];
  const portfolio = buyer?.builder_portfolio ?? detail?.builder_portfolio;
  const sectionIds = useMemo(() => {
    const ids: PreviewSectionId[] = ["registration"];
    if (overviewFacts.length > 0 || extraOverviewClaims.length > 0) ids.push("overview");
    if (scheduleFacts.length > 0) ids.push("schedule");
    if (quarterlySeries && quarterlySurface) ids.push("quarterly");
    if (planItems.length > 0) ids.push("plans");
    if (complaints.length > 0) ids.push("complaints");
    if (portfolio) ids.push("builder");
    return ids;
  }, [complaints.length, extraOverviewClaims.length, overviewFacts.length, planItems.length, portfolio, quarterlySeries, quarterlySurface, scheduleFacts.length]);
  const phaseIndex = useLandingLoopSequence({
    active,
    durations: sectionIds.map(() => SECTION_DURATION_MS),
    paused,
    reducedMotion,
  });
  const activeSectionId = sectionIds[phaseIndex] ?? sectionIds[0];

  useEffect(() => {
    if (!active || !activeSectionId) return;
    const viewport = viewportRef.current;
    const target = viewport?.querySelector<HTMLElement>(`[data-landing-rera-section="${activeSectionId}"]`);
    if (!viewport || !target) return;
    viewport.scrollTo({
      top: activeSectionId === "registration" ? 0 : Math.max(0, target.offsetTop - 14),
      behavior: reducedMotion ? "auto" : "smooth",
    });
  }, [active, activeSectionId, reducedMotion]);

  return (
    <div className="landing-product landing-product--record landing-rera-report" data-section={activeSectionId}>
      <div className="landing-rera-report__viewport" ref={viewportRef}>
        <article className="rera-report-page landing-rera-report__page">
          <header className="rera-hero" data-landing-rera-section="registration">
            <span className="landing-rera-report__eyebrow">Karnataka RERA</span>
            <h1>{homeName(property)}</h1>
            <p>{[property.area, "Bengaluru", status ? humanize(status) : null].filter(Boolean).join(" · ")}</p>
            {registration ? (
              <div className="rera-registry-line">
                <strong>{registration}</strong>
                {httpUrl(buyer?.registry_url) ? (
                  <a href={httpUrl(buyer?.registry_url)!} target="_blank" rel="noreferrer">Open registry</a>
                ) : null}
              </div>
            ) : null}
          </header>

          {(overviewFacts.length > 0 || extraOverviewClaims.length > 0) ? (
            <ReportSection id="overview" title="Project at a glance">
              <dl className="rera-metric-grid">
                {overviewFacts.map((fact) => (
                  <div key={`${fact.key}:${fact.value}`}><dt>{fact.label}</dt><dd>{buyerFactValue(fact)}</dd></div>
                ))}
                {extraOverviewClaims.map((fact) => (
                  <div key={fact.id}><dt>{fact.label}</dt><dd>{fact.value}</dd></div>
                ))}
              </dl>
            </ReportSection>
          ) : null}

          {scheduleFacts.length > 0 ? (
            <ReportSection id="schedule" title="Schedule and progress">
              <ol className="rera-timeline">
                {scheduleFacts.map((fact) => (
                  <li key={`${fact.key}:${fact.value}`}><span>{fact.label}</span><strong>{buyerFactValue(fact)}</strong></li>
                ))}
              </ol>
            </ReportSection>
          ) : null}

          {quarterlySeries && quarterlySurface ? (
            <ReportSection id="quarterly" title="Quarterly progress">
              <div className="rera-series" role="region" aria-label="Quarterly progress" tabIndex={0}>
                <table>
                  <thead><tr><th>Filing</th><th>{quarterlyLabels.booked_units ?? "Booked"}</th><th>{quarterlyLabels.unsold_units ?? "Unsold"}</th><th>{quarterlyLabels.total_units ?? "Filed homes"}</th></tr></thead>
                  <tbody>
                    {quarterlySeries.points.map((point) => {
                      const total = point.total_units ?? 0;
                      const booked = point.booked_units ?? 0;
                      return (
                        <tr key={point.point_id}>
                          <th scope="row"><strong>{[point.quarter, point.financial_year].filter(Boolean).join(" · ")}</strong><span>{formatReraDate(point.effective_at)}</span></th>
                          <td><strong>{booked.toLocaleString("en-IN")}</strong>{total > 0 ? <progress max={total} value={booked} aria-label={`${booked} of ${total} homes filed as booked`} /> : null}</td>
                          <td>{point.unsold_units?.toLocaleString("en-IN") ?? "—"}</td>
                          <td>{point.total_units?.toLocaleString("en-IN") ?? "—"}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </ReportSection>
          ) : null}

          {planItems.length > 0 ? (
            <ReportSection id="plans" title={plansSurface?.title ?? "Plans"}>
              <div className="rera-plan-grid">
                {planItems.map((plan) => (
                  <a className="rera-plan-preview" href={plan.previewUrl} target="_blank" rel="noreferrer" key={plan.id}>
                    <img src={plan.previewUrl} alt="" />
                    <strong>{plan.label}</strong>
                    {plan.detail ? <span>{plan.detail}</span> : null}
                  </a>
                ))}
              </div>
            </ReportSection>
          ) : null}

          {complaints.length > 0 ? (
            <ReportSection id="complaints" title="Complaints and orders">
              <div className="rera-complaint-groups">
                {complaints.map((complaint) => (
                  <article key={complaint.scope || "complaints"}>
                    <h3>{complaint.scope === "promoter" ? "Promoter" : "Project"}</h3>
                    <dl className="rera-metric-grid">
                      <div><dt>Recorded</dt><dd>{complaint.total.toLocaleString("en-IN")}</dd></div>
                      <div><dt>Open</dt><dd>{complaint.open.toLocaleString("en-IN")}</dd></div>
                      <div><dt>Disposed</dt><dd>{complaint.disposed.toLocaleString("en-IN")}</dd></div>
                    </dl>
                  </article>
                ))}
              </div>
            </ReportSection>
          ) : null}

          {portfolio ? (
            <ReportSection id="builder" title={`${portfolio.builder_name} record`}>
              <dl className="rera-metric-grid">
                <div><dt>Tracked projects</dt><dd>{portfolio.tracked_projects.toLocaleString("en-IN")}</dd></div>
                <div><dt>RERA linked</dt><dd>{portfolio.rera_registered_projects.toLocaleString("en-IN")}</dd></div>
                <div><dt>Delayed</dt><dd>{portfolio.delayed_projects.toLocaleString("en-IN")}</dd></div>
              </dl>
            </ReportSection>
          ) : null}
        </article>
      </div>
      <div className="landing-rera-report__position" aria-hidden="true">
        {sectionIds.map((id, index) => <span key={id} className={index === phaseIndex ? "is-active" : ""} />)}
      </div>
    </div>
  );
}
