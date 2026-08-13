import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { PageState } from "../components/PageState.tsx";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import {
  claimValueText,
  claimsForSelector,
  displayFactsForSection,
  formatReraDate,
  httpUrl,
  orderReraDocuments,
  orderReraRegulatoryEvents,
  previewReraRegulatoryEvents,
  regulatoryCoverageNote,
  regulatoryEventPresentation,
  selectReraPlanPreviews,
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
  | { status: "error"; message: string };

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
              {presentation.source && (
                <a href={presentation.source.source_url} target="_blank" rel="noreferrer">
                  {presentation.actionLabel}
                </a>
              )}
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

function canonicalDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value.trim().toLowerCase() : date.toISOString().slice(0, 10);
}

function Schedule({
  section,
  surface,
  evidence,
}: {
  section?: ReraBuyerFactSection;
  surface?: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
}) {
  const facts = section?.facts ?? [];
  const seenDates = new Set(facts.map((fact) => canonicalDate(fact.value)));
  const claimFacts = surface
    ? displayFactsForSection(surface, evidence)
      .filter((fact) => !seenDates.has(canonicalDate(fact.value)))
    : [];
  if (facts.length === 0 && claimFacts.length === 0) return null;
  return (
    <Section id="schedule" title="Schedule and progress">
      <ol className="rera-timeline">
        {facts.map((fact) => (
          <li key={`${fact.key}:${fact.value}`}>
            <span>{fact.label}</span>
            <strong>{buyerFactValue(fact)}</strong>
          </li>
        ))}
        {claimFacts.map((fact) => (
          <li key={fact.id}>
            <span>{fact.label}</span>
            <time dateTime={fact.value}>{formatReraDate(fact.value)}</time>
          </li>
        ))}
      </ol>
    </Section>
  );
}

function QuarterlyProgress({
  section,
  evidence,
}: {
  section?: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
}) {
  const series = evidence.series.find((item) => item.series_type === "quarterly_inventory");
  if (!series || !section) return null;
  const labels = Object.fromEntries(section.selectors.map((selector) => [selector.key.split(".").at(-1), selector.label]));
  return (
    <Section id="quarterly-progress" title="Quarterly progress">
      <div className="rera-series" role="region" aria-label="Quarterly progress" tabIndex={0}>
        <table>
          <thead>
            <tr>
              <th>Filing</th>
              <th>{labels.booked_units ?? "Booked"}</th>
              <th>{labels.unsold_units ?? "Unsold"}</th>
              <th>{labels.total_units ?? "Filed homes"}</th>
            </tr>
          </thead>
          <tbody>
            {series.points.map((point) => {
              const total = point.total_units ?? 0;
              const booked = point.booked_units ?? 0;
              return (
                <tr key={point.point_id}>
                  <th scope="row">
                    <strong>{[point.quarter, point.financial_year].filter(Boolean).join(" · ")}</strong>
                    <span>{formatReraDate(point.effective_at)}</span>
                  </th>
                  <td>
                    <strong>{booked.toLocaleString("en-IN")}</strong>
                    {total > 0 && <progress max={total} value={booked} aria-label={`${booked} of ${total} homes filed as booked`} />}
                  </td>
                  <td>{point.unsold_units?.toLocaleString("en-IN") ?? "—"}</td>
                  <td>{point.total_units?.toLocaleString("en-IN") ?? "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
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
  const entities = evidence.entities.filter((entity) => entity.entity_type === "inventory_configuration");
  const valueSelectors = section.selectors.filter(({ key }) => (
    key.startsWith("claim:")
    && entities.some((entity) => claimsForSelector(evidence.claims, key, entity.entity_id).length > 0)
  ));
  const rows = new Map<string, { entity: (typeof entities)[number]; claims: ReraEvidenceClaim[] }>();
  for (const entity of entities) {
    const claims = evidence.claims.filter((claim) => claim.subject.entity_id === entity.entity_id);
    const values = valueSelectors.map((selector) => {
      const claim = claimsForSelector(claims, selector.key)[0];
      return claim ? claimValueText(claim.value, selector.format) : "—";
    });
    const normalizedLabel = (entity.label ?? "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "");
    rows.set(`${normalizedLabel}:${values.join("|")}`, { entity, claims });
  }
  if (rows.size === 0) return null;
  return (
    <Section id="inventory" title="Homes and carpet area">
      <div className="rera-table-wrap" role="region" aria-label="Homes and carpet area" tabIndex={0}>
        <table className="rera-table">
          <thead>
            <tr>
              <th>Configuration</th>
              {valueSelectors.map((selector) => <th key={selector.key}>{selector.label}</th>)}
            </tr>
          </thead>
          <tbody>
            {[...rows.values()].map(({ entity, claims: rowClaims }) => {
              return (
                <tr key={entity.entity_id}>
                  <th scope="row">{entity.label ?? "Filed configuration"}</th>
                  {valueSelectors.map((selector) => {
                    const claim = claimsForSelector(rowClaims, selector.key)[0];
                    return <td key={selector.key}>{claim ? claimValueText(claim.value, selector.format) : "—"}</td>;
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Section>
  );
}

function Discrepancies({ evidence }: { evidence: ReraEvidenceProjection }) {
  const comparisons = evidence.discrepancies.flatMap((item) => item.comparisons)
    .filter((comparison) => comparison.relationship === "different_values");
  if (comparisons.length === 0) return null;
  return (
    <Section id="filed-differences" title="Filed totals to compare">
      <div className="rera-discrepancies">
        {comparisons.map((comparison) => (
          <div key={comparison.id}>
            <strong>Differing filed totals</strong>
            <span>
              {[
                ...comparison.left.map((measurement) => measurement.value),
                ...comparison.right.map((measurement) => measurement.value),
              ].filter((value, index, values) => values.indexOf(value) === index)
                .map((value) => `${value.toLocaleString("en-IN")} ${comparison.unit === "square_metres" ? "m²" : comparison.unit}`)
                .join(" vs ")}
            </span>
          </div>
        ))}
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
  const otherProjects = portfolio?.projects.filter((project) => !project.current) ?? [];
  if (!portfolio || otherProjects.length === 0) return null;
  return (
    <Section id="builder" title={`More ${portfolio.builder_name} projects in this catalog`}>
      <div className="rera-table-wrap" role="region" aria-label={`${portfolio.builder_name} projects`} tabIndex={0}>
        <table className="rera-table rera-builder-table">
          <thead>
            <tr><th>Project</th><th>Registration</th><th>Current target</th><th>Status</th></tr>
          </thead>
          <tbody>
            {otherProjects.map((project) => {
              const registryUrl = httpUrl(project.rera_portal_url);
              return (
                <tr key={`${project.property_id}:${project.rera_number ?? project.project_name}`}>
                  <th scope="row">
                    <Link to={`/property/${encodeURIComponent(project.property_id)}`}>{project.project_name}</Link>
                    <span>{project.area}{project.current ? " · This project" : ""}</span>
                  </th>
                  <td>
                    {registryUrl && project.rera_number
                      ? <a href={registryUrl} target="_blank" rel="noreferrer">{project.rera_number}</a>
                      : project.rera_number ?? "—"}
                  </td>
                  <td>{formatMonth(project.completion_date)}</td>
                  <td>{builderProjectState(project)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
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
                    <summary>Example filed subjects</summary>
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
  if (value?.startsWith("/media/")) return value;
  return httpUrl(value);
}

type PlanPreviewItem = {
  id: string;
  label: string;
  previewUrl: string;
  detail?: string;
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
                {activePreview.detail && <p>{activePreview.detail}</p>}
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
      if (document.group.toLowerCase() !== "plans") unique.set(document.url, document);
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
      .map(([key, group]) => ({ ...group, key, items: orderReraDocuments(group.items) }))
      .sort((left, right) => right.items.length - left.items.length || left.label.localeCompare(right.label));
  }, [documents, evidence]);

  if (grouped.length === 0) return null;

  return (
    <Section id="documents" title={surface?.title ?? "Approvals and documents"}>
      <div className="rera-document-groups">
        {grouped.map(({ key, label, items }) => (
          <details key={key}>
            <summary>
              <span>{label}</span>
              <strong>{items.length.toLocaleString("en-IN")} {items.length === 1 ? "document" : "documents"}</strong>
            </summary>
            <ul>
              {items.map((document) => (
                <li key={`${document.id}:${document.url}`}>
                  <a href={document.url} target="_blank" rel="noreferrer">{document.label}</a>
                </li>
              ))}
            </ul>
          </details>
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

  useEffect(() => {
    let active = true;
    Promise.all([getProperty(id), getPropertyRera(id)])
      .then(([detail, report]) => active && setState({ status: "ready", detail, report }))
      .catch((error: unknown) => active && setState({
        status: "error",
        message: error instanceof Error ? error.message : "RERA record could not be loaded.",
      }));
    return () => { active = false; };
  }, [id]);

  const latestCapture = useMemo(() => {
    if (state.status !== "ready") return undefined;
    return state.report.evidence.regulatory_coverage
      .map((coverage) => coverage.checked_at)
      .sort()
      .at(-1);
  }, [state]);

  if (state.status === "loading") return <PageState variant="loading" context="property" />;
  if (state.status === "error") return <PageState variant="error" context="property" message={state.message} />;

  const { detail, report } = state;
  const property = detail.property;
  const title = state.detail.society?.name.trim() || property.title.trim();
  const buyer = report.buyer_report;
  const factSections = buyer?.fact_sections;
  const registration = sectionById(factSections, "registration");
  const registrationNumber = report.evidence.claims.find((claim) => claim.predicate === "official_registration_number");
  const registrationText = registrationNumber
    ? claimValueText(registrationNumber.value)
    : registration?.facts.find((fact) => /registration number/i.test(fact.label))?.value;
  const status = registration?.facts.find((fact) => fact.key === "rera_status")?.value;
  const statusLabel = status ? humanize(status) : undefined;
  const registryUrl = httpUrl(buyer?.registry_url);
  const hasReportData = report.evidence.claims.length > 0 || (factSections?.some((section) => section.facts.length) ?? false);

  return (
    <main className="page-container-wide rera-report-page">
      <Helmet>
        <title>{title} RERA - OpenEstates</title>
        <meta name="description" content={`Filed project details for ${title}.`} />
      </Helmet>
      <header className="rera-hero">
        <Link to={`/property/${encodeURIComponent(id)}`}>Back to property</Link>
        <h1>{title}</h1>
        <p>{[property.area, property.city, statusLabel, latestCapture ? `Updated ${formatReraDate(latestCapture)}` : null].filter(Boolean).join(" · ")}</p>
        {(registrationText || registryUrl) && (
          <div className="rera-registry-line">
            {registrationText && <strong>{registrationText}</strong>}
            {registryUrl && <a href={registryUrl} target="_blank" rel="noreferrer">Open registry</a>}
          </div>
        )}
        {report.availability === "partial" && <span>Some filing sections are not available.</span>}
      </header>

      {!hasReportData ? (
        <section className="rera-empty">
          <h2>RERA record unavailable</h2>
          <p>No matched filing is available for this property yet.</p>
        </section>
      ) : (
        <>
          <RegulatoryRecord report={report} />
          <ProjectOverview
            section={sectionById(factSections, "overview")}
            surface={surfaceById(report.surface.sections, "overview")}
            evidence={report.evidence}
          />
          <Discrepancies evidence={report.evidence} />
          <Schedule
            section={sectionById(factSections, "schedule")}
            surface={surfaceById(report.surface.sections, "schedule")}
            evidence={report.evidence}
          />
          <QuarterlyProgress section={surfaceById(report.surface.sections, "quarterly_progress")} evidence={report.evidence} />
          <Inventory section={surfaceById(report.surface.sections, "inventory")} evidence={report.evidence} />
          <Plans
            plans={detail.plans}
            surface={surfaceById(report.surface.sections, "plans")}
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
          <Documents
            documents={buyer?.documents ?? []}
            evidence={report.evidence}
            surface={surfaceById(report.surface.sections, "documents")}
          />
          <FiledSchedules sections={buyer?.schedules ?? []} />
          {sectionById(factSections, "location")?.facts.length ? (
            <Section id="location" title="Registered location">
              <BuyerFactList facts={sectionById(factSections, "location")!.facts} />
            </Section>
          ) : null}
          <BuilderRecord portfolio={buyer?.builder_portfolio} />
        </>
      )}
    </main>
  );
}
