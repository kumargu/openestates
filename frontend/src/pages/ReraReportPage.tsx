import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import type {
  BuilderPortfolio,
  BuilderProjectRecord,
  PropertyDetailResponse,
  ReraComplaintSection,
  ReraDecisionCard,
  ReraDossier,
  ReraLegalCheck,
  ReraReportFact,
  ReraReportSection,
  ReraScheduleSection,
  ReraTimeline,
} from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { NotebookPinButton } from "../components/notebook/NotebookPinButton.tsx";
import { LinkIcon } from "../components/evidence/EvidenceIcons.tsx";
import {
  displayName,
  httpUrl,
  kindLabel,
  knownText,
  presentLocationFacts,
  reportSections,
  safeLabels,
  toneClass,
  visibleDocumentSections,
} from "../lib/reraReportView.ts";

type LoadState =
  | { status: "ready"; id: string; detail: PropertyDetailResponse; dossier: ReraDossier }
  | { status: "error"; id: string; message: string };

function formatDate(value?: string): string | null {
  const known = knownText(value);
  if (!known) return null;
  const date = new Date(known);
  if (Number.isNaN(date.getTime())) return known;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

function formatCheckedDate(value?: string): string | null {
  const known = knownText(value);
  if (!known) return null;
  const date = new Date(known);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

function compactNumber(value: number): string {
  return value.toLocaleString("en-IN");
}

function countLabel(count: number, noun: string): string {
  return `${compactNumber(count)} ${noun}${count === 1 ? "" : "s"}`;
}

function areaLabel(value?: number): string | null {
  if (value == null || !Number.isFinite(value)) return null;
  return `${value.toLocaleString("en-IN", { maximumFractionDigits: 2 })} sqm`;
}

function firstKnown(values: Array<string | null | undefined>): string | null {
  for (const value of values) {
    const known = knownText(value);
    if (known) return known;
  }
  return null;
}

function buyerDetail(value: string): string {
  return value
    .replace(/\s*·\s*parsed with caveats/gi, "")
    .replace(/\bparsed with caveats\b/gi, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function sectionById(sections: ReraReportSection[], id: string): ReraReportSection | null {
  return sections.find((section) => section.id === id) ?? null;
}

function factTone(tone?: string): "attention" | "clear" | "neutral" {
  if (tone === "risk" || tone === "watch" || tone === "caution") return "attention";
  if (tone === "positive") return "clear";
  return "neutral";
}

function factToneLabel(tone?: string): string {
  const value = factTone(tone);
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function FactPin({
  fact,
  propertyId,
  sectionId,
}: {
  fact: ReraReportFact;
  propertyId: string;
  sectionId: string;
}) {
  return (
    <NotebookPinButton
      propertyId={propertyId}
      catalogKey={`rera-report:${propertyId}:${sectionId}:${fact.key}:${fact.value}`}
      title={`${fact.label}: ${fact.value}`}
      source="RERA"
      labels={safeLabels(fact.labels, fact.key)}
      className="rera-report-pin"
    />
  );
}

function SectionHead({
  title,
  action,
}: {
  title: string;
  action?: ReactNode;
}) {
  return (
    <div className="rera-report-section__head">
      <h2>{title}</h2>
      {action}
    </div>
  );
}

function RecordRows({
  facts,
  propertyId,
  sectionId,
}: {
  facts: ReraReportFact[];
  propertyId: string;
  sectionId: string;
}) {
  if (facts.length === 0) return null;

  return (
    <dl className="rera-report-record">
      {facts.map((fact) => (
        <div
          key={`${sectionId}-${fact.key}-${fact.value}`}
          className={`rera-report-record__row ${toneClass(fact.tone)}`.trim()}
        >
          <dt>{fact.label}</dt>
          <dd>{fact.value}</dd>
          <span className="rera-report-record__pin">
            <FactPin fact={fact} propertyId={propertyId} sectionId={sectionId} />
          </span>
        </div>
      ))}
    </dl>
  );
}

function LocationSection({
  section,
  propertyId,
}: {
  section: ReraReportSection;
  propertyId: string;
}) {
  const { address, coordinates, coordinatesDisplay, otherFacts } = presentLocationFacts(section.facts);
  if (!address && !coordinates && otherFacts.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-location-section" id="rera-location">
      <SectionHead title={section.title} />
      {(address || coordinatesDisplay) && (
        <div className="rera-report-location">
          <div className="rera-report-location__row">
            <div className="rera-report-location__copy">
              {address && <p className="rera-report-location__address">{address.value}</p>}
              {coordinatesDisplay && (
                <p className="rera-report-location__coords">{coordinatesDisplay}</p>
              )}
            </div>
            {address && <FactPin fact={address} propertyId={propertyId} sectionId={section.id} />}
            {!address && coordinates && (
              <NotebookPinButton
                propertyId={propertyId}
                catalogKey={`rera-report:${propertyId}:${section.id}:${coordinates.key}:${coordinates.value}`}
                title={`Coordinates: ${coordinatesDisplay}`}
                source="RERA"
                labels={safeLabels(coordinates.labels, coordinates.key)}
                className="rera-report-pin"
              />
            )}
          </div>
        </div>
      )}
      <RecordRows facts={otherFacts} propertyId={propertyId} sectionId={section.id} />
    </section>
  );
}

function ReraSummary({
  cards,
}: {
  cards: ReraDecisionCard[];
}) {
  const visible = cards
    .filter((card) => knownText(card.title) || knownText(card.detail))
    .slice(0, 6);

  if (visible.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-summary" id="rera-summary">
      <SectionHead title="At a glance" />
      <div className="rera-report-summary__rows">
        {visible.map((card) => (
          <div key={card.id} className={`rera-report-summary__row ${toneClass(card.tone)}`.trim()}>
            <span>{factToneLabel(card.tone)}</span>
            <strong>{card.title}</strong>
            {card.detail && <p>{buyerDetail(card.detail)}</p>}
          </div>
        ))}
      </div>
    </section>
  );
}

function DocumentSectionList({
  sections,
  propertyId,
}: {
  sections: ReraDossier["document_sections"];
  propertyId: string;
}) {
  const visibleSections = visibleDocumentSections(sections);
  const [activeGroup, setActiveGroup] = useState("all");

  if (visibleSections.length === 0) return null;

  const total = visibleSections.reduce((sum, section) => sum + section.items.length, 0);
  const resolvedActiveGroup = activeGroup === "all"
    || visibleSections.some((section) => section.group === activeGroup)
    ? activeGroup
    : "all";
  const selectedSections = resolvedActiveGroup === "all"
    ? visibleSections
    : visibleSections.filter((section) => section.group === resolvedActiveGroup);

  return (
    <section className="rera-report-section rera-report-documents" id="rera-documents">
      <SectionHead title="Documents" />
      <div className="rera-report-tabs" role="tablist" aria-label="Document groups">
        <button
          type="button"
          role="tab"
          aria-selected={resolvedActiveGroup === "all"}
          className={resolvedActiveGroup === "all" ? "is-active" : ""}
          onClick={() => setActiveGroup("all")}
        >
          <span>All</span>
          <strong>{total}</strong>
        </button>
        {visibleSections.map((section) => (
          <button
            key={section.group}
            type="button"
            role="tab"
            aria-selected={resolvedActiveGroup === section.group}
            className={resolvedActiveGroup === section.group ? "is-active" : ""}
            onClick={() => setActiveGroup(section.group)}
          >
            <span>{section.label}</span>
            <strong>{section.items.length}</strong>
          </button>
        ))}
      </div>

      <div className="rera-report-document-table">
        {selectedSections.map((section) => (
          <div key={section.group} className="rera-report-document-group">
            <h3>{section.label}</h3>
            <div className="rera-report-document-links">
              {section.items.map((item) => {
                const href = httpUrl(item.source_url);
                if (!href) return null;
                const itemLabel = knownText(item.label) ?? kindLabel(item.kind);
                const detail = knownText(item.source_field_label) ?? kindLabel(item.kind);
                return (
                  <div key={`${section.group}-${item.artifact_id || href}`} className="rera-report-document-link">
                    <a href={href} target="_blank" rel="noreferrer">
                      <LinkIcon size={14} />
                      <span>{itemLabel}</span>
                    </a>
                    <small>{detail}</small>
                    <div className="rera-report-document-link__actions">
                      <NotebookPinButton
                        propertyId={propertyId}
                        catalogKey={`rera-document:${propertyId}:${section.group}:${item.artifact_id || href}`}
                        title={`${section.label}: ${itemLabel}`}
                        source="RERA"
                        labels={["legal"]}
                        className="rera-report-pin"
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function ComplaintTabs({
  sections,
}: {
  sections: ReraComplaintSection[];
}) {
  const visible = sections.filter((section) => section.total > 0 || section.open > 0 || section.disposed > 0);
  const [activeScope, setActiveScope] = useState(visible[0]?.scope ?? "");

  if (visible.length === 0) return null;

  const resolvedActiveScope = visible.some((section) => section.scope === activeScope)
    ? activeScope
    : visible[0]!.scope;
  const active = visible.find((section) => section.scope === resolvedActiveScope) ?? visible[0]!;

  return (
    <section className="rera-report-section rera-report-complaints" id="rera-complaints">
      <SectionHead title="Complaints" />
      <div className="rera-report-tabs" role="tablist" aria-label="Complaint scopes">
        {visible.map((section) => (
          <button
            key={section.scope}
            type="button"
            role="tab"
            aria-selected={active.scope === section.scope}
            className={active.scope === section.scope ? "is-active" : ""}
            onClick={() => setActiveScope(section.scope)}
          >
            <span>{section.label}</span>
            <strong>{section.total}</strong>
          </button>
        ))}
      </div>

      <div className="rera-report-complaint-read">
        <dl>
          <div>
            <dt>Total</dt>
            <dd>{compactNumber(active.total)}</dd>
          </div>
          <div className={active.open > 0 ? "is-watch" : ""}>
            <dt>Open</dt>
            <dd>{compactNumber(active.open)}</dd>
          </div>
          <div>
            <dt>Disposed</dt>
            <dd>{compactNumber(active.disposed)}</dd>
          </div>
        </dl>

        {active.top_themes.length > 0 && (
          <div className="rera-report-theme-list">
            {active.top_themes.slice(0, 8).map((theme) => (
              <span key={`${active.scope}-${theme.label}`}>
                {theme.label} · {compactNumber(theme.count)}
              </span>
            ))}
          </div>
        )}

        {active.sample_subjects.length > 0 && (
          <ul className="rera-report-list">
            {active.sample_subjects.slice(0, 4).map((subject) => (
              <li key={subject}>{subject}</li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function TimelineSection({
  timeline,
}: {
  timeline: ReraTimeline;
}) {
  const items = [
    { label: "Start", value: formatDate(timeline.start_date) },
    { label: "Original target", value: formatDate(timeline.original_completion_date) },
    { label: "Current target", value: formatDate(timeline.completion_date) },
    {
      label: "Movement",
      value: timeline.delay_months && timeline.delay_months > 0
        ? `${timeline.delay_months} months`
        : null,
      tone: timeline.delay_months && timeline.delay_months > 0 ? "watch" : "neutral",
    },
  ].filter((item) => item.value);

  if (items.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-timeline" id="rera-timeline">
      <SectionHead title="Schedule" />
      <div className="rera-report-metrics">
        {items.map((item) => (
          <div key={item.label} className={toneClass(item.tone)}>
            <span>{item.label}</span>
            <strong>{item.value}</strong>
          </div>
        ))}
      </div>
    </section>
  );
}

function LegalChecks({
  checks,
}: {
  checks: ReraLegalCheck[];
}) {
  const visible = checks.filter((check) => knownText(check.value));
  if (visible.length === 0) return null;

  return (
    <section className="rera-report-section" id="rera-legal">
      <SectionHead title="Legal / finance" />
      <dl className="rera-report-record">
        {visible.map((check) => (
          <div key={check.key} className={`rera-report-record__row ${toneClass(check.tone)}`.trim()}>
            <dt>{check.label}</dt>
            <dd>{check.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function BuilderRecord({
  portfolio,
  promoterComplaints,
}: {
  portfolio?: BuilderPortfolio | null;
  promoterComplaints?: ReraComplaintSection | null;
}) {
  if (!portfolio && !promoterComplaints) return null;

  const projects = portfolio?.projects.slice(0, 10) ?? [];

  return (
    <section className="rera-report-section rera-report-builder" id="rera-builder">
      <SectionHead title="Builder record" />
      <div className="rera-report-metrics">
        {portfolio && (
          <>
            <div>
              <span>Tracked projects</span>
              <strong>{compactNumber(portfolio.tracked_projects)}</strong>
            </div>
            <div>
              <span>RERA linked</span>
              <strong>{portfolio.rera_registered_projects}/{portfolio.tracked_projects}</strong>
            </div>
            <div className={portfolio.delayed_projects > 0 ? "is-watch" : ""}>
              <span>Delayed</span>
              <strong>{compactNumber(portfolio.delayed_projects)}</strong>
            </div>
            <div className={portfolio.revocations && portfolio.revocations > 0 ? "is-watch" : ""}>
              <span>Revocations</span>
              <strong>{portfolio.revocations ?? "0"}</strong>
            </div>
          </>
        )}
        {promoterComplaints && (
          <div className={promoterComplaints.open > 0 ? "is-watch" : ""}>
            <span>Promoter complaints</span>
            <strong>
              {compactNumber(promoterComplaints.total)}
              {promoterComplaints.open > 0 ? ` · ${compactNumber(promoterComplaints.open)} open` : ""}
            </strong>
          </div>
        )}
      </div>

      {projects.length > 0 && (
        <div className="rera-report-table-wrap">
          <table className="rera-report-table">
            <thead>
              <tr>
                <th>Project</th>
                <th>RERA</th>
                <th>Target</th>
                <th>Complaints</th>
              </tr>
            </thead>
            <tbody>
              {projects.map((project) => (
                <BuilderProjectRow key={`${project.property_id}-${project.rera_number ?? project.project_name}`} project={project} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function BuilderProjectRow({
  project,
}: {
  project: BuilderProjectRecord;
}) {
  const href = project.rera_portal_url ? httpUrl(project.rera_portal_url) : null;
  return (
    <tr className={project.current ? "is-current" : ""}>
      <td>
        <Link to={`/property/${project.property_id}`}>{project.project_name}</Link>
        <span>{project.area}{project.current ? " · This home" : ""}</span>
      </td>
      <td>
        {href && project.rera_number ? (
          <a href={href} target="_blank" rel="noreferrer">{project.rera_number}</a>
        ) : (
          <span>{project.rera_number ?? project.rera_status ?? "—"}</span>
        )}
      </td>
      <td className={project.delay_months && project.delay_months > 0 ? "is-watch" : ""}>
        {formatDate(project.completion_date) ?? project.project_status_display ?? "—"}
        {project.delay_months && project.delay_months > 0 ? <span>{project.delay_months} mo movement</span> : null}
      </td>
      <td>{project.complaints_count != null ? compactNumber(project.complaints_count) : "—"}</td>
    </tr>
  );
}

function ProjectFactsSection({
  section,
  propertyId,
}: {
  section: ReraReportSection | null;
  propertyId: string;
}) {
  if (!section || section.facts.length === 0) return null;
  return (
    <section className="rera-report-section" id="rera-project">
      <SectionHead title="Project specs" />
      <RecordRows facts={section.facts} propertyId={propertyId} sectionId={section.id} />
    </section>
  );
}

function ReraSchedules({
  sections,
}: {
  sections: ReraScheduleSection[];
}) {
  const visible = sections
    .map((section) => ({
      ...section,
      rows: section.rows.filter((row) => knownText(row.label)),
    }))
    .filter((section) => section.rows.length > 0);

  if (visible.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-schedules" id="rera-schedules">
      <SectionHead title="RERA schedules" />
      <div className="rera-report-schedule-groups">
        {visible.map((section) => (
          <div key={section.group} className="rera-report-schedule-group">
            <h3>{section.label}</h3>
            <dl className="rera-report-record">
              {section.rows.map((row) => {
                const area = areaLabel(row.area_sqm);
                const state = row.available === true ? "Yes" : row.available === false ? "No" : null;
                return (
                  <div key={`${section.group}-${row.label}`} className="rera-report-record__row">
                    <dt>{row.label}</dt>
                    <dd>{[state, area, row.value].filter(Boolean).join(" · ")}</dd>
                  </div>
                );
              })}
            </dl>
          </div>
        ))}
      </div>
    </section>
  );
}

function CompleteFacts({
  sections,
  propertyId,
}: {
  sections: ReraReportSection[];
  propertyId: string;
}) {
  const visible = sections.filter((section) => section.facts.length > 0);
  if (visible.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-complete" id="rera-complete">
      <SectionHead title="Complete record" />
      <div className="rera-report-complete__groups">
        {visible.map((section) => (
          <details key={section.id} className="rera-report-complete__group" open={section.id === "registration"}>
            <summary>
              <span>{section.title}</span>
              <strong>{countLabel(section.facts.length, "fact")}</strong>
            </summary>
            <RecordRows facts={section.facts} propertyId={propertyId} sectionId={section.id} />
          </details>
        ))}
      </div>
    </section>
  );
}

export function ReraReportPage() {
  const { id } = useParams<{ id: string }>();
  const [state, setState] = useState<LoadState | null>(null);

  useEffect(() => {
    if (!id) return;

    const propertyId = id;
    const controller = new AbortController();
    Promise.all([
      getProperty(propertyId, { signal: controller.signal }),
      getPropertyRera(propertyId),
    ])
      .then(([detail, dossier]) => {
        setState({ status: "ready", id: propertyId, detail, dossier });
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setState({
          status: "error",
          id: propertyId,
          message: error instanceof Error ? error.message : "Unable to load RERA report.",
        });
      });

    return () => controller.abort();
  }, [id]);

  const currentState = state?.id === id ? state : null;
  const sections = useMemo(() => (
    currentState?.status === "ready" ? reportSections(currentState.dossier) : []
  ), [currentState]);

  if (!id) return <PageState variant="not_found" context="property" />;
  if (!currentState) return <PageState variant="loading" context="property" message="Opening RERA." />;
  if (currentState.status === "error") return <PageState variant="error" context="property" message={currentState.message} />;

  const property = currentState.detail.property;
  const dossier = currentState.dossier;
  const title = displayName(property.title);
  const sourceUrl = httpUrl(dossier.source.portal_url);
  const registrationNumber = knownText(dossier.source.registration_number);
  const checked = formatCheckedDate(dossier.source.last_verified);
  const pageTitle = `${title} RERA - OpenEstates`;
  const locationSection = sectionById(sections, "location");
  const projectSection = sectionById(sections, "project");
  const promoterComplaints = dossier.complaint_sections.find((section) => section.scope === "promoter")
    ?? dossier.complaint_sections.find((section) => /promoter|builder/i.test(section.label))
    ?? null;
  const heroMeta = [
    property.area,
    property.city,
    firstKnown([dossier.source.status, dossier.source.registered ? "Registered" : null]),
    checked ? `Checked ${checked}` : null,
  ].filter((value): value is string => Boolean(value));

  return (
    <div className="page-container-wide rera-report-page">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={`RERA facts for ${title}.`} />
      </Helmet>

      <header className="rera-report-hero">
        <Link to={`/property/${encodeURIComponent(id)}`} className="rera-report-back">
          Back to property
        </Link>
        <p>RERA</p>
        <h1>{title}</h1>
        {heroMeta.length > 0 && (
          <div className="rera-report-subline">
            {heroMeta.map((item) => <span key={item}>{item}</span>)}
          </div>
        )}
        {(registrationNumber || sourceUrl) && (
          <div className="rera-report-registry">
            {registrationNumber && (
              <div className="rera-report-registry__number">
                <span>{registrationNumber}</span>
              </div>
            )}
            {sourceUrl && (
              <a href={sourceUrl} target="_blank" rel="noreferrer">
                <LinkIcon size={14} />
                Open RERA
              </a>
            )}
          </div>
        )}
      </header>

      <ReraSummary cards={dossier.summary_cards} />

      <DocumentSectionList
        sections={dossier.document_sections ?? []}
        propertyId={property.id}
      />

      <ComplaintTabs sections={dossier.complaint_sections ?? []} />

      <BuilderRecord
        portfolio={currentState.detail.builder_portfolio}
        promoterComplaints={promoterComplaints}
      />

      <TimelineSection timeline={dossier.timeline} />

      <LegalChecks checks={dossier.legal_checks ?? []} />

      <ProjectFactsSection section={projectSection} propertyId={property.id} />

      <ReraSchedules sections={dossier.schedule_sections ?? []} />

      {locationSection && (
        <LocationSection section={locationSection} propertyId={property.id} />
      )}

      <CompleteFacts sections={sections} propertyId={property.id} />
    </div>
  );
}
