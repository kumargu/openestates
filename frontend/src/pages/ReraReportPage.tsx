import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { PageState } from "../components/PageState.tsx";
import { PageTitle } from "../components/PageTitle.tsx";
import { SaveHeartButton } from "../components/SaveHeartButton.tsx";
import { useSearchSpan } from "../components/workspace/SearchSpanContext.ts";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import { propertyHrefWithSearchSpan } from "../lib/navigationContext.ts";
import { backendUrl } from "../lib/runtimeConfig.ts";
import {
  claimValueText,
  buildReraReportViewModel,
  displayFactsForSection,
  formatReraDate,
  httpUrl,
  orderReraDocuments,
  orderReraRegulatoryEvents,
  previewReraRegulatoryEvents,
  projectReraInventoryChart,
  regulatoryCoverageNote,
  regulatoryEventPresentation,
  selectReraPlanPreviews,
} from "../lib/reraReportView.ts";
import type {
  ReraModuleState,
  ReraReportViewModel,
} from "../lib/reraReportView.ts";
import type {
  BuilderPortfolio,
  BuilderProjectRecord,
  PropertyDetailResponse,
  ProjectPlansView,
  ReraBuyerComplaintSummary,
  ReraBuyerDocument,
  ReraBuyerFact,
  ReraBuyerFactSection,
  ReraEvidenceClaim,
  ReraEvidenceProjection,
  ReraEvidenceReportResponse,
  ReraReportSurfaceSection,
  ReraScheduleSection,
} from "../lib/types.ts";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; detail: PropertyDetailResponse; report: ReraEvidenceReportResponse }
  | { status: "error" };

function sectionById(
  sections: ReraBuyerFactSection[] | undefined,
  id: string,
): ReraBuyerFactSection | undefined {
  return sections?.find((section) => section.id === id);
}

function surfaceById(
  sections: ReraReportSurfaceSection[],
  id: string,
): ReraReportSurfaceSection | undefined {
  return sections.find((section) => section.id === id);
}

function humanize(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/(^|\s)\S/g, (letter) => letter.toUpperCase());
}

function statusTone(value: string): "positive" | "risk" | "neutral" {
  if (/revoked|rejected|cancelled|suspended|expired/i.test(value)) return "risk";
  if (/approved|active|registered|valid/i.test(value)) return "positive";
  return "neutral";
}

function moduleStateLabel(state: ReraModuleState): string {
  switch (state) {
    case "available": return "Found";
    case "partial": return "Partial";
    case "stale": return "Stale";
    case "conflicting": return "Review";
    case "missing": return "Not found";
    case "not_applicable": return "Not due";
  }
}

function ReraRecordHeader({
  propertyId,
  title,
  location,
  model,
}: {
  propertyId: string;
  title: string;
  location: string;
  model: ReraReportViewModel;
}) {
  const searchSpan = useSearchSpan();
  let matchState = "Registry record matched";
  if (model.registrations.length === 0) matchState = "Registration match unresolved";
  else if (model.state === "partial") matchState = "Partial registry match";
  else if (model.state === "conflicting") matchState = "Registry match · Review differences";
  else if (model.state === "stale") matchState = "Registry match · Stale";
  return (
    <header className="rera-record-header">
      <div className="rera-record-header__actions">
        <Link to={propertyHrefWithSearchSpan(propertyId, searchSpan)}><span aria-hidden="true">←</span> Property</Link>
        <div>
          <SaveHeartButton propertyId={propertyId} label="Save record" />
          {model.registryUrl && (
            <a href={model.registryUrl} target="_blank" rel="noreferrer">Official registry <span aria-hidden="true">↗</span></a>
          )}
        </div>
      </div>
      <div className="rera-record-header__title">
        <div>
          <span>RERA project record</span>
          <h1>{title}</h1>
          {location && <p>{location}</p>}
        </div>
        <div className="rera-record-header__meta">
          <strong className={`rera-evidence-state is-${model.state}`}>{matchState}</strong>
          {model.checkedAt && <time dateTime={model.checkedAt}>Checked {formatReraDate(model.checkedAt)}</time>}
        </div>
      </div>
    </header>
  );
}

function ReraDecisionSummary({ model }: { model: ReraReportViewModel }) {
  return (
    <section className="rera-summary" aria-labelledby="rera-summary-title">
      <div className="rera-section-heading">
        <span>One-minute record</span>
        <h2 id="rera-summary-title">What the filing shows</h2>
      </div>
      <div className="rera-summary-grid">
        {model.summary.map((fact) => (
          <article className={`rera-summary-card is-${fact.state}`} key={fact.id}>
            <div>
              <span>{fact.label}</span>
              <small>{moduleStateLabel(fact.state)}</small>
            </div>
            <strong>{fact.value}</strong>
            {fact.detail && <p>{fact.detail}</p>}
          </article>
        ))}
      </div>
    </section>
  );
}

function RegistrationList({ model }: { model: ReraReportViewModel }) {
  if (model.registrations.length === 0) {
    return (
      <Section id="registrations" title="Registrations by phase">
        <div className="rera-compact-state">
          <strong>Registration match unresolved</strong>
          <span>The available record does not identify an exact registration.</span>
        </div>
      </Section>
    );
  }
  return (
    <Section id="registrations" title="Registrations by phase">
      <div className="rera-registration-list">
        {model.registrations.map((registration) => (
          <article key={registration.id}>
            <header>
              <div>
                <span>Phase or scope</span>
                <h3>{registration.scope}</h3>
              </div>
              {registration.status && (
                <strong className={`rera-status is-${statusTone(registration.status)}`}>
                  <span aria-hidden="true" />{humanize(registration.status)}
                </strong>
              )}
            </header>
            <dl>
              <div><dt>Registration number</dt><dd>{registration.number ?? "Not in record"}</dd></div>
              <div><dt>Declared homes</dt><dd>{registration.units ?? "Not in record"}</dd></div>
              <div><dt>Current completion</dt><dd>{registration.completion ? formatReraDate(registration.completion) : "Not in record"}</dd></div>
            </dl>
          </article>
        ))}
      </div>
    </Section>
  );
}

function DeliveryAndProgress({ model }: { model: ReraReportViewModel }) {
  if (model.delivery.length === 0 && model.quarterlyFilings.length === 0) return null;
  const registrationScope = (registrationId?: string) => model.registrations.length > 1
    ? model.registrations.find((registration) => registration.id === registrationId)?.scope
    : undefined;
  return (
    <Section id="progress" title="Delivery and quarterly progress">
      <div className="rera-progress-workspace">
        {model.delivery.length > 0 && (
          <article className="rera-progress-module">
            <header>
              <span>Delivery movement</span>
              <small>Official dates</small>
            </header>
            <ol className="rera-delivery-list">
              {model.delivery.map((item, index) => (
                <li key={item.id}>
                  <span aria-hidden="true">{index + 1}</span>
                  <div>
                    <small>{item.label}</small>
                    <strong>{formatReraDate(item.value)}</strong>
                    {registrationScope(item.registrationId) && <small>{registrationScope(item.registrationId)}</small>}
                  </div>
                </li>
              ))}
            </ol>
          </article>
        )}
        {model.quarterlyFilings.length > 0 && (
          <article className="rera-progress-module">
            <header>
              <span>Quarterly filings</span>
              <small>Promoter reported</small>
            </header>
            <ol className="rera-quarter-list">
              {model.quarterlyFilings.map((filing, index) => (
                <li key={filing.id}>
                  <div>
                    <strong>{filing.period}</strong>
                    {index === 0 && <span>Latest</span>}
                    <time dateTime={filing.filedAt}>{formatReraDate(filing.filedAt)}</time>
                    {registrationScope(filing.registrationId) && <small>{registrationScope(filing.registrationId)}</small>}
                  </div>
                  <dl>
                    <div><dt>Booked</dt><dd>{filing.bookedUnits?.toLocaleString("en-IN") ?? "—"}</dd></div>
                    <div><dt>Unsold</dt><dd>{filing.unsoldUnits?.toLocaleString("en-IN") ?? "—"}</dd></div>
                    <div><dt>Filed homes</dt><dd>{filing.totalUnits?.toLocaleString("en-IN") ?? "—"}</dd></div>
                  </dl>
                </li>
              ))}
            </ol>
          </article>
        )}
      </div>
    </Section>
  );
}

function RecordCoverage({ model }: { model: ReraReportViewModel }) {
  return (
    <Section id="coverage" title="Record completeness">
      <div className="rera-coverage-index">
        {model.coverage.map((item) => (
          <div key={item.id}>
            <span className={`rera-coverage-dot is-${item.state}`} aria-hidden="true" />
            <div><strong>{item.label}</strong><span>{item.detail}</span></div>
            <small>{moduleStateLabel(item.state)}</small>
          </div>
        ))}
        {model.checkedAt && (
          <div>
            <span className="rera-coverage-dot is-available" aria-hidden="true" />
            <div><strong>Registry check</strong><span>{formatReraDate(model.checkedAt)}</span></div>
            <small>Checked</small>
          </div>
        )}
      </div>
    </Section>
  );
}

function buyerFactValue(fact: ReraBuyerFact): string {
  const key = fact.key.toLowerCase();
  return key.endsWith("_date") || key.endsWith("_on")
    ? formatReraDate(fact.value)
    : fact.value;
}

function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section className="rera-section" id={`rera-${id}`}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function BuyerFactList({ facts }: { facts: ReraBuyerFact[] }) {
  if (facts.length === 0) return null;
  return (
    <dl className="rera-fact-list">
      {facts.map((fact) => (
        <div key={`${fact.key}:${fact.value}`}>
          <dt>{fact.label}</dt>
          <dd><strong>{buyerFactValue(fact)}</strong></dd>
        </div>
      ))}
    </dl>
  );
}

function ClaimFactList({
  section,
  evidence,
}: {
  section?: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
}) {
  if (!section) return null;
  const facts = displayFactsForSection(section, evidence);
  if (facts.length === 0) return null;
  return (
    <dl className="rera-fact-list">
      {facts.map((fact) => (
        <div key={fact.id}>
          <dt>{fact.label}</dt>
          <dd>
            <strong>{fact.value}</strong>
            {fact.claims[0]?.assertion_mode !== "registry_record" && <span>{fact.assertion}</span>}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function ProjectOverview({
  section,
  surface,
  evidence,
}: {
  section?: ReraBuyerFactSection;
  surface?: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
}) {
  const claimFacts = surface ? displayFactsForSection(surface, evidence) : [];
  const buyerFacts = section?.facts ?? [];
  if (buyerFacts.length === 0 && claimFacts.length === 0) return null;
  const knownValues = new Set(buyerFacts.map((fact) => fact.value.trim().toLowerCase()));
  const extraClaims = claimFacts.filter((fact) => !knownValues.has(fact.value.trim().toLowerCase()));
  return (
    <Section id="overview" title="Project at a glance">
      <dl className="rera-metric-grid">
        {buyerFacts.map((fact) => (
          <div key={`${fact.key}:${fact.value}`}>
            <dt>{fact.label}</dt>
            <dd>{buyerFactValue(fact)}</dd>
          </div>
        ))}
        {extraClaims.map((fact) => (
          <div key={fact.id}>
            <dt>{fact.label}</dt>
            <dd>{fact.value}</dd>
          </div>
        ))}
      </dl>
    </Section>
  );
}

function RegulatoryRecord({ report }: { report: ReraEvidenceReportResponse }) {
  const [expanded, setExpanded] = useState(false);
  const section = surfaceById(report.surface.sections, "regulatory_record");
  if (!section || report.evidence.events.length === 0) return null;

  const events = orderReraRegulatoryEvents(
    report.evidence.events,
    report.surface.regulatory_event_order,
  );
  const limit = section.items_per_page ?? 3;
  const visibleEvents = expanded
    ? events
    : previewReraRegulatoryEvents(
      report.evidence.events,
      report.surface.regulatory_event_order,
      limit,
    );
  const coverageNote = regulatoryCoverageNote(
    report.evidence.regulatory_coverage,
    report.surface.coverage_note,
  );

  return (
    <Section id="regulatory-record" title={section.title}>
      <ol className="rera-regulatory-events">
        {visibleEvents.map((event) => {
          const presentation = regulatoryEventPresentation(event, report.evidence);
          return (
            <li key={event.event_id}>
              <div className="rera-regulatory-event-copy">
                <strong>{event.current_effect}</strong>
                <span>
                  {[event.issuer, formatReraDate(event.occurred_at), event.disposition ? humanize(event.disposition) : null]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
                {presentation.supportingEvidence?.supporting_quote && (
                  <blockquote>
                    “{presentation.supportingEvidence.supporting_quote}”
                    {presentation.supportingEvidence.page && (
                      <cite>Page {presentation.supportingEvidence.page}</cite>
                    )}
                  </blockquote>
                )}
              </div>
            </li>
          );
        })}
      </ol>
      {events.length > limit && (
        <button
          className="rera-chronology-toggle"
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? "Show current items" : "Show full chronology"}
        </button>
      )}
      {coverageNote && (
        <p className="rera-regulatory-coverage">{coverageNote}</p>
      )}
    </Section>
  );
}

function Inventory({
  section,
  evidence,
}: {
  section?: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
}) {
  if (!section) return null;
  const rows = projectReraInventoryChart(section, evidence);
  if (rows.length === 0) return null;
  return (
    <Section id="inventory" title="Homes and carpet area">
      <div className="rera-inventory-chart" role="table">
        <div className="rera-inventory-chart__header" role="row">
          <span role="columnheader">Configuration</span>
          <span role="columnheader">Homes</span>
          <span role="columnheader">Avg carpet / home</span>
        </div>
        <div role="rowgroup">
          {rows.map((row) => (
            <div className="rera-inventory-chart__row" role="row" key={row.id}>
              <strong role="rowheader">{row.label}</strong>
              <div className="rera-inventory-chart__measure is-homes" role="cell">
                <b className="rera-inventory-chart__mobile-label" aria-hidden="true">Homes</b>
                <svg viewBox="0 0 100 6" preserveAspectRatio="none" aria-hidden="true">
                  <rect className="rera-inventory-chart__track" width="100" height="6" rx="3" />
                  <path className="rera-inventory-chart__grid" d="M25 0V6 M50 0V6 M75 0V6" />
                  <rect className="rera-inventory-chart__bar" width={row.homesPercent} height="6" rx="3" />
                </svg>
                <span>{row.homesDisplay}</span>
              </div>
              <div className="rera-inventory-chart__measure is-area" role="cell">
                <b className="rera-inventory-chart__mobile-label" aria-hidden="true">Avg carpet</b>
                <svg viewBox="0 0 100 6" preserveAspectRatio="none" aria-hidden="true">
                  <rect className="rera-inventory-chart__track" width="100" height="6" rx="3" />
                  <path className="rera-inventory-chart__grid" d="M25 0V6 M50 0V6 M75 0V6" />
                  <rect className="rera-inventory-chart__bar" width={row.carpetAreaPerHomePercent} height="6" rx="3" />
                </svg>
                <span>{row.carpetAreaPerHomeDisplay}</span>
                {row.carpetAreaLabel?.toLowerCase().startsWith("filed") && <small>Filed</small>}
              </div>
            </div>
          ))}
        </div>
      </div>
    </Section>
  );
}

function formatMonth(value?: string): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-IN", { month: "short", year: "numeric" }).format(date);
}

function builderProjectState(project: BuilderProjectRecord): string {
  if (project.delay_months != null && project.delay_months > 0) {
    return `${project.delay_months} month movement`;
  }
  const status = project.project_status_display ?? project.rera_status;
  return status ? humanize(status) : "—";
}

function BuilderRecord({ portfolio }: { portfolio?: BuilderPortfolio }) {
  const searchSpan = useSearchSpan();
  const [expanded, setExpanded] = useState(false);
  const otherProjects = portfolio?.projects.filter((project) => !project.current) ?? [];
  if (!portfolio || otherProjects.length === 0) return null;
  const previewLimit = 6;
  const visibleProjects = expanded ? otherProjects : otherProjects.slice(0, previewLimit);
  return (
    <Section id="builder" title={`Other projects by ${portfolio.builder_name}`}>
      <ol className="rera-builder-index">
        {visibleProjects.map((project) => (
          <li key={`${project.property_id}:${project.rera_number ?? project.project_name}`}>
            <div className="rera-builder-index__identity">
              <Link to={propertyHrefWithSearchSpan(project.property_id, searchSpan)}>{project.project_name}</Link>
              <span>{project.area}</span>
              {project.rera_number && <small>{project.rera_number}</small>}
            </div>
            <dl>
              <div><dt>Current target</dt><dd>{formatMonth(project.completion_date)}</dd></div>
              <div><dt>Record state</dt><dd>{builderProjectState(project)}</dd></div>
            </dl>
          </li>
        ))}
      </ol>
      {otherProjects.length > previewLimit && (
        <button
          className="rera-list-toggle"
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? "Show fewer projects" : `Show ${otherProjects.length - previewLimit} more projects`}
        </button>
      )}
    </Section>
  );
}

function Complaints({
  complaints,
  fallback,
}: {
  complaints: ReraBuyerComplaintSummary[];
  fallback?: ReraBuyerFactSection;
}) {
  if (complaints.length === 0 && !fallback?.facts.length) return null;
  return (
    <Section id="complaints" title="Complaints and orders">
      {complaints.length > 0 ? (
        <div className="rera-complaint-groups">
          {complaints.map((complaint) => {
            const themes = Object.entries(complaint.theme_counts)
              .filter(([, count]) => count > 0)
              .sort((left, right) => right[1] - left[1])
              .slice(0, 4);
            const subjects = complaint.sample_subjects?.filter((subject) => subject.trim()).slice(0, 3) ?? [];
            const statusValue = (count: number) => {
              if (complaint.status_counts_complete) return count.toLocaleString("en-IN");
              return count > 0 ? `${count.toLocaleString("en-IN")}+` : "—";
            };
            return (
              <article key={complaint.scope || "complaints"}>
                <h3>{complaint.scope === "promoter" ? "Promoter" : "Project"}</h3>
                <dl className="rera-metric-grid">
                  <div><dt>Recorded</dt><dd>{complaint.total.toLocaleString("en-IN")}</dd></div>
                  <div><dt>Open</dt><dd>{statusValue(complaint.open)}</dd></div>
                  <div><dt>Disposed</dt><dd>{statusValue(complaint.disposed)}</dd></div>
                </dl>
                {!complaint.status_counts_complete && complaint.rows_parsed > 0 && (
                  <p className="rera-complaint-coverage">
                    Status and themes cover {complaint.rows_parsed.toLocaleString("en-IN")} returned cases.
                  </p>
                )}
                {themes.length > 0 && (
                  <ul className="rera-theme-list">
                    {themes.map(([theme, count]) => (
                      <li key={theme}>{humanize(theme)} · {count.toLocaleString("en-IN")}</li>
                    ))}
                  </ul>
                )}
                {subjects.length > 0 && (
                  <details className="rera-complaint-subjects">
                    <summary>Sample complaint subjects</summary>
                    <ul>{subjects.map((subject) => <li key={subject}>{subject}</li>)}</ul>
                  </details>
                )}
              </article>
            );
          })}
        </div>
      ) : (
        <BuyerFactList facts={fallback?.facts ?? []} />
      )}
    </Section>
  );
}

function planPreviewUrl(value?: string): string | null {
  if (value?.startsWith("/media/")) return backendUrl(value);
  return httpUrl(value);
}

type PlanPreviewItem = {
  id: string;
  label: string;
  previewUrl: string;
  page: number | undefined;
  detail: string | undefined;
};

function Plans({
  plans,
  surface,
}: {
  plans?: ProjectPlansView;
  surface?: ReraReportSurfaceSection;
}) {
  const [activePreviewIndex, setActivePreviewIndex] = useState<number | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const siteOverview = plans?.site_overview;
  const floorPlans = plans?.floor_plans ?? [];
  const filedPlanPreviews = selectReraPlanPreviews(
    plans?.filed_plan_previews ?? [],
    surface?.preview_kinds ?? [],
  );
  const previewItems = [
    siteOverview && planPreviewUrl(siteOverview.preview_url)
      ? {
          id: siteOverview.artifact_id,
          label: siteOverview.label,
          previewUrl: planPreviewUrl(siteOverview.preview_url)!,
          page: siteOverview.page,
          detail: undefined,
        }
      : null,
    ...floorPlans.map((plan) => {
      const previewUrl = planPreviewUrl(plan.preview_url);
      if (!previewUrl) return null;
      const areas = [
        plan.carpet_area_sqft ? `${plan.carpet_area_sqft.toLocaleString("en-IN")} sq ft carpet` : null,
        plan.sale_area_sqft ? `${plan.sale_area_sqft.toLocaleString("en-IN")} sq ft sale area` : null,
      ].filter(Boolean).join(" · ");
      return {
        id: plan.artifact_id,
        label: plan.title,
        previewUrl,
        page: plan.page,
        detail: areas || undefined,
      };
    }),
    ...filedPlanPreviews.map((plan) => {
      const previewUrl = planPreviewUrl(plan.preview_url);
      if (!previewUrl) return null;
      return {
        id: plan.artifact_id,
        label: plan.label,
        previewUrl,
        page: plan.page,
        detail: undefined,
      };
    }),
  ].filter((item): item is PlanPreviewItem => Boolean(item));
  const visiblePreviewItems = previewItems.slice(
    0,
    surface?.items_per_page ?? previewItems.length,
  );
  const safeActiveIndex = activePreviewIndex == null
    ? null
    : Math.min(activePreviewIndex, visiblePreviewItems.length - 1);
  const activePreview = safeActiveIndex == null ? null : visiblePreviewItems[safeActiveIndex];

  useEffect(() => {
    if (!activePreview) return undefined;
    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setActivePreviewIndex(null);
    };

    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    closeButtonRef.current?.focus();
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
      previouslyFocused?.focus();
    };
  }, [activePreview]);

  if (visiblePreviewItems.length === 0) return null;

  return (
    <Section id="plans" title={surface?.title ?? "Plans"}>
      <div className="rera-plan-grid">
        {visiblePreviewItems.map((plan, index) => (
          <button
            type="button"
            className="rera-plan-preview"
            key={plan.id}
            aria-haspopup="dialog"
            aria-label={`Open ${plan.label} preview`}
            onClick={() => setActivePreviewIndex(index)}
          >
            <img src={plan.previewUrl} alt="" />
            <strong>{plan.label}</strong>
            {plan.detail && <span>{plan.detail}</span>}
          </button>
        ))}
      </div>
      {activePreview && (
        <div
          className="rera-plan-lightbox-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setActivePreviewIndex(null);
          }}
        >
          <div
            className="rera-plan-lightbox"
            role="dialog"
            aria-modal="true"
            aria-labelledby="rera-plan-lightbox-title"
          >
            <header>
              <div>
                <h2 id="rera-plan-lightbox-title">{activePreview.label}</h2>
                {(activePreview.detail || activePreview.page) && (
                  <p>{[activePreview.detail, activePreview.page ? `Page ${activePreview.page}` : null].filter(Boolean).join(" · ")}</p>
                )}
              </div>
              <div className="rera-plan-lightbox-actions">
                <a href={activePreview.previewUrl} download>Download image</a>
                <button
                  ref={closeButtonRef}
                  type="button"
                  aria-label="Close plan preview"
                  onClick={() => setActivePreviewIndex(null)}
                >
                  ×
                </button>
              </div>
            </header>
            <div className="rera-plan-lightbox-image">
              <img src={activePreview.previewUrl} alt={activePreview.label} />
            </div>
            {visiblePreviewItems.length > 1 && (
              <div className="rera-plan-lightbox-strip" aria-label="Other plan previews">
                {visiblePreviewItems.map((plan, index) => (
                  <button
                    type="button"
                    className={index === safeActiveIndex ? "is-active" : undefined}
                    key={plan.id}
                    aria-label={`Show ${plan.label}`}
                    aria-pressed={index === safeActiveIndex}
                    onClick={() => setActivePreviewIndex(index)}
                  >
                    <img src={plan.previewUrl} alt="" />
                    <span>{plan.label}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </Section>
  );
}

function ReraDocumentGroup({
  label,
  items,
  pageSize,
}: {
  label: string;
  items: ReraBuyerDocument[];
  pageSize: number;
}) {
  const [page, setPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(items.length / pageSize));
  const safePage = Math.min(page, pageCount - 1);
  const visibleItems = items.slice(safePage * pageSize, (safePage + 1) * pageSize);
  return (
    <details>
      <summary>
        <span>{label}</span>
        <strong>{items.length.toLocaleString("en-IN")} {items.length === 1 ? "document" : "documents"}</strong>
      </summary>
      <ul>
        {visibleItems.map((document) => (
          <li key={`${document.id}:${document.url}`}>
            <a href={document.url} target="_blank" rel="noreferrer">{document.label}</a>
          </li>
        ))}
      </ul>
      {pageCount > 1 && (
        <div className="rera-document-pager" aria-label={`${label} pages`}>
          <button type="button" disabled={safePage === 0} onClick={() => setPage((value) => Math.max(0, value - 1))}>Previous</button>
          <span>{safePage + 1} / {pageCount}</span>
          <button type="button" disabled={safePage === pageCount - 1} onClick={() => setPage((value) => Math.min(pageCount - 1, value + 1))}>Next</button>
        </div>
      )}
    </details>
  );
}

function Documents({
  documents,
  evidence,
  surface,
}: {
  documents: ReraBuyerDocument[];
  evidence: ReraEvidenceProjection;
  surface?: ReraReportSurfaceSection;
}) {
  const grouped = useMemo(() => {
    const filingDocuments = evidence.entities
      .filter((entity) => entity.entity_type === "document")
      .flatMap((entity): ReraBuyerDocument[] => {
        const claims = evidence.claims.filter((claim) => claim.subject.entity_id === entity.entity_id);
        const urlClaim = claims.find((claim) => claim.predicate === "official_document_url");
        if (urlClaim?.value.type !== "document_ref") return [];
        const url = httpUrl(urlClaim.value.data);
        if (!url) return [];
        const period = ["document_quarter", "document_financial_year"]
          .map((predicate) => claims.find((claim) => claim.predicate === predicate))
          .filter((claim): claim is ReraEvidenceClaim => Boolean(claim))
          .map((claim) => claimValueText(claim.value))
          .filter(Boolean)
          .join(" · ");
        return [{
          id: entity.entity_id,
          label: [period, entity.label ?? "Filed document"].filter(Boolean).join(" · "),
          group: "quarterly_filings",
          group_label: "Quarterly filings",
          url,
        }];
      });
    const unique = new Map<string, ReraBuyerDocument>();
    for (const document of [...documents, ...filingDocuments]) {
      unique.set(document.url, document);
    }
    const groups = new Map<string, { label: string; items: ReraBuyerDocument[] }>();
    for (const document of unique.values()) {
      const key = document.group || "documents";
      const group = groups.get(key);
      groups.set(key, {
        label: document.group_label || humanize(key),
        items: [...(group?.items ?? []), document],
      });
    }
    return [...groups.entries()]
      .map(([key, group]) => ({ ...group, key, items: orderReraDocuments(group.items) }));
  }, [documents, evidence]);

  if (grouped.length === 0) return null;

  return (
    <Section id="documents" title={surface?.title ?? "Approvals and documents"}>
      <div className="rera-document-groups">
        {grouped.map(({ key, label, items }) => (
          <ReraDocumentGroup
            key={key}
            label={label}
            items={items}
            pageSize={surface?.items_per_page ?? 10}
          />
        ))}
      </div>
    </Section>
  );
}

function FiledSchedules({ sections }: { sections: ReraScheduleSection[] }) {
  const visible = sections
    .map((section) => ({ ...section, rows: section.rows.filter((row) => row.label.trim()) }))
    .filter((section) => section.rows.length > 0);
  if (visible.length === 0) return null;
  return (
    <Section id="schedules" title="Filed project schedules">
      <div className="rera-schedule-groups">
        {visible.map((section) => (
          <details key={section.group || section.label}>
            <summary>{section.label || humanize(section.group)}</summary>
            <dl className="rera-fact-list">
              {section.rows.map((row) => {
                const values = [
                  row.available == null ? null : row.available ? "Yes" : "No",
                  row.area_sqm == null ? null : `${row.area_sqm.toLocaleString("en-IN", { maximumFractionDigits: 1 })} m²`,
                  row.value,
                ].filter(Boolean).join(" · ");
                return <div key={`${section.group}:${row.label}`}><dt>{row.label}</dt><dd><strong>{values}</strong></dd></div>;
              })}
            </dl>
          </details>
        ))}
      </div>
    </Section>
  );
}

function Declarations({
  report,
  finance,
  water,
}: {
  report: ReraEvidenceReportResponse;
  finance?: ReraBuyerFactSection;
  water?: ReraBuyerFactSection;
}) {
  const financeSurface = surfaceById(report.surface.sections, "finance");
  const waterSurface = surfaceById(report.surface.sections, "water");
  const hasFinance = Boolean(finance?.facts.length || (financeSurface && displayFactsForSection(financeSurface, report.evidence).length));
  const hasWater = Boolean(water?.facts.length || (waterSurface && displayFactsForSection(waterSurface, report.evidence).length));
  if (!hasFinance && !hasWater) return null;
  return (
    <Section id="declarations" title="Filed declarations">
      <div className="rera-declaration-groups">
        {hasFinance && (
          <div>
            <h3>Legal and financial</h3>
            {finance?.facts.length
              ? <BuyerFactList facts={finance.facts} />
              : <ClaimFactList section={financeSurface} evidence={report.evidence} />}
          </div>
        )}
        {hasWater && (
          <div>
            <h3>Water and services</h3>
            {water?.facts.length
              ? <BuyerFactList facts={water.facts} />
              : <ClaimFactList section={waterSurface} evidence={report.evidence} />}
          </div>
        )}
      </div>
    </Section>
  );
}

export function ReraReportPage() {
  const { id = "" } = useParams();
  return <ReraReportContent key={id} id={id} />;
}

function ReraReportContent({ id }: { id: string }) {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [retryKey, setRetryKey] = useState(0);

  useEffect(() => {
    let active = true;
    Promise.all([getProperty(id), getPropertyRera(id)])
      .then(([detail, report]) => active && setState({ status: "ready", detail, report }))
      .catch(() => active && setState({ status: "error" }));
    return () => { active = false; };
  }, [id, retryKey]);

  if (state.status === "loading") return <PageState variant="loading" context="property" />;
  if (state.status === "error") {
    return (
      <PageState
        variant="error"
        context="property"
        onRetry={() => {
          setState({ status: "loading" });
          setRetryKey((current) => current + 1);
        }}
      />
    );
  }

  const { detail, report } = state;
  const property = detail.property;
  const title = state.detail.society?.name.trim() || property.title.trim();
  const buyer = report.buyer_report;
  const factSections = buyer?.fact_sections;
  const model = buildReraReportViewModel(report);
  const builderFacts = sectionById(factSections, "builder")?.facts ?? [];
  const promoterFacts = builderFacts.filter((fact) => fact.key !== "rera_promoter_name");

  return (
    <main className="page-container-wide rera-report-page">
      <PageTitle title={`${title} RERA | ${PUBLIC_BRAND_NAME}`} />
      <meta name="description" content={`Filed project details for ${title}.`} />
      <ReraRecordHeader
        propertyId={id}
        title={title}
        location={[property.area, property.city].filter(Boolean).join(" · ")}
        model={model}
      />

      {!model.hasData ? (
        <section className="rera-empty">
          <h2>RERA record unavailable</h2>
          <p>No matched filing is available for this property.</p>
        </section>
      ) : (
        <>
          <ReraDecisionSummary model={model} />
          <nav className="rera-section-index" aria-label="RERA record sections">
            <a href="#rera-registrations">Registrations</a>
            {(model.delivery.length > 0 || model.quarterlyFilings.length > 0) && <a href="#rera-progress">Progress</a>}
            {(buyer?.documents?.length ?? 0) > 0 && <a href="#rera-documents">Documents</a>}
            {(buyer?.complaints?.length ?? 0) > 0 && <a href="#rera-complaints">Complaints</a>}
            <a href="#rera-coverage">Completeness</a>
          </nav>
          <RegistrationList model={model} />
          <DeliveryAndProgress model={model} />
          <RegulatoryRecord report={report} />
          <ProjectOverview
            section={sectionById(factSections, "overview")}
            surface={surfaceById(report.surface.sections, "overview")}
            evidence={report.evidence}
          />
          <Inventory section={surfaceById(report.surface.sections, "inventory")} evidence={report.evidence} />
          <Plans
            plans={detail.plans}
            surface={surfaceById(report.surface.sections, "plans")}
          />
          <Documents
            documents={buyer?.documents ?? []}
            evidence={report.evidence}
            surface={surfaceById(report.surface.sections, "documents")}
          />
          <Complaints
            complaints={buyer?.complaints ?? []}
            fallback={sectionById(factSections, "complaints")}
          />
          <Declarations
            report={report}
            finance={sectionById(factSections, "finance")}
            water={sectionById(factSections, "water")}
          />
          <FiledSchedules sections={buyer?.schedules ?? []} />
          {sectionById(factSections, "location")?.facts.length ? (
            <Section id="location" title="Registered location">
              <BuyerFactList facts={sectionById(factSections, "location")!.facts} />
            </Section>
          ) : null}
          {promoterFacts.length > 0 && (
            <Section id="promoter" title="Promoter record">
              <BuyerFactList facts={promoterFacts} />
            </Section>
          )}
          <BuilderRecord portfolio={buyer?.builder_portfolio} />
          <RecordCoverage model={model} />
        </>
      )}
    </main>
  );
}
