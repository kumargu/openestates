import { useEffect, useMemo, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { PageState } from "../components/PageState.tsx";
import { PlanGallery } from "../components/property/PlanGallery.tsx";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import {
  assertionLabel,
  claimValueText,
  claimsForSelector,
  displayFactsForSection,
  formatReraDate,
  httpUrl,
  sectionHasEvidence,
  selectorMatches,
} from "../lib/reraReportView.ts";
import type {
  PropertyDetailResponse,
  ReraEvidenceClaim,
  ReraEvidenceProjection,
  ReraEvidenceReportResponse,
  ReraReportSurfaceSection,
} from "../lib/types.ts";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; detail: PropertyDetailResponse; report: ReraEvidenceReportResponse }
  | { status: "error"; message: string };

function EvidenceButton({ claims, onOpen }: {
  claims: ReraEvidenceClaim[];
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  if (claims.length === 0) return null;
  return (
    <button type="button" className="rera-source-button" onClick={() => onOpen(claims)}>
      Source
    </button>
  );
}

function FactList({
  section,
  evidence,
  onOpen,
}: {
  section: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
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
          <EvidenceButton claims={fact.claims} onOpen={onOpen} />
        </div>
      ))}
    </dl>
  );
}

function Timeline({
  section,
  evidence,
  onOpen,
}: {
  section: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  const events = evidence.events.filter((event) => section.selectors.some(({ key }) => (
    selectorMatches(key, `event:${event.event_type}`)
  )));
  if (events.length === 0) return null;
  return (
    <ol className="rera-timeline">
      {events.map((event) => {
        const selector = section.selectors.find(({ key }) => selectorMatches(key, `event:${event.event_type}`));
        const claims = evidence.claims.filter((claim) => event.claim_ids.includes(claim.claim_id));
        return (
          <li key={event.event_id}>
            <time dateTime={event.date}>{formatReraDate(event.date)}</time>
            <strong>{selector?.label ?? event.event_type}</strong>
            <EvidenceButton claims={claims} onOpen={onOpen} />
          </li>
        );
      })}
    </ol>
  );
}

function QuarterlySeries({
  section,
  evidence,
  onOpen,
}: {
  section: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  const series = evidence.series.find((item) => item.series_type === "quarterly_inventory");
  if (!series) return null;
  const labels = Object.fromEntries(section.selectors.map((selector) => [selector.key.split(".").at(-1), selector.label]));
  return (
    <div className="rera-series" role="region" aria-label={section.title} tabIndex={0}>
      <table>
        <thead>
          <tr>
            <th>Filing</th>
            <th>{labels.booked_units ?? "Booked"}</th>
            <th>{labels.unsold_units ?? "Unsold"}</th>
            <th>{labels.total_units ?? "Filed homes"}</th>
            <th><span className="sr-only">Evidence</span></th>
          </tr>
        </thead>
        <tbody>
          {series.points.map((point) => {
            const claims = evidence.claims.filter((claim) => point.claim_ids.includes(claim.claim_id));
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
                <td><EvidenceButton claims={claims} onOpen={onOpen} /></td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function InventoryTable({
  section,
  evidence,
  onOpen,
}: {
  section: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  const entities = evidence.entities.filter((entity) => entity.entity_type === "inventory_configuration");
  const valueSelectors = section.selectors.filter(({ key }) => (
    key.startsWith("claim:")
    && entities.some((entity) => claimsForSelector(evidence.claims, key, entity.entity_id).length > 0)
  ));
  const registrationClaims = evidence.claims.filter((claim) => claim.subject.entity_type === "registration");
  const summaryFacts = displayFactsForSection(section, { ...evidence, claims: registrationClaims });
  if (entities.length === 0 && summaryFacts.length === 0) return null;
  return (
    <>
      {summaryFacts.length > 0 && (
        <dl className="rera-inline-facts">
          {summaryFacts.map((fact) => (
            <div key={fact.id}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
              <EvidenceButton claims={fact.claims} onOpen={onOpen} />
            </div>
          ))}
        </dl>
      )}
      {entities.length > 0 && (
        <div className="rera-table-wrap" role="region" aria-label={section.title} tabIndex={0}>
          <table className="rera-table">
            <thead>
              <tr>
                <th>Configuration</th>
                {valueSelectors.map((selector) => <th key={selector.key}>{selector.label}</th>)}
                <th><span className="sr-only">Evidence</span></th>
              </tr>
            </thead>
            <tbody>
              {entities.map((entity) => {
                const rowClaims = evidence.claims.filter((claim) => claim.subject.entity_id === entity.entity_id);
                return (
                  <tr key={entity.entity_id}>
                    <th scope="row">{entity.label ?? "Filed configuration"}</th>
                    {valueSelectors.map((selector) => {
                      const claim = claimsForSelector(rowClaims, selector.key)[0];
                      return <td key={selector.key}>{claim ? claimValueText(claim.value, selector.format) : "—"}</td>;
                    })}
                    <td><EvidenceButton claims={rowClaims} onOpen={onOpen} /></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <Discrepancies evidence={evidence} onOpen={onOpen} />
    </>
  );
}

function Discrepancies({ evidence, onOpen }: {
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  const comparisons = evidence.discrepancies.flatMap((item) => item.comparisons)
    .filter((comparison) => comparison.relationship === "different_values");
  if (comparisons.length === 0) return null;
  return (
    <div className="rera-discrepancies">
      {comparisons.map((comparison) => {
        const claims = evidence.claims.filter((claim) => comparison.input_claim_ids.includes(claim.claim_id));
        const unit = comparison.unit === "square_metres" ? "m²" : comparison.unit;
        return (
          <div key={comparison.id}>
            <strong>Differing filed totals</strong>
            <span>{comparison.observed_deltas.map((delta) => `${Math.abs(delta).toLocaleString("en-IN")} ${unit}`).join(", ")}</span>
            <EvidenceButton claims={claims} onOpen={onOpen} />
          </div>
        );
      })}
    </div>
  );
}

function Documents({
  evidence,
  onOpen,
}: {
  evidence: ReraEvidenceProjection;
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  const documents = evidence.entities.filter((entity) => entity.entity_type === "document");
  if (documents.length === 0) return null;
  return (
    <div className="rera-documents">
      {documents.map((document) => {
        const claims = evidence.claims.filter((claim) => claim.subject.entity_id === document.entity_id);
        const urlClaim = claims.find((claim) => claim.predicate === "official_document_url");
        const quarter = claims.find((claim) => claim.predicate === "document_quarter");
        const year = claims.find((claim) => claim.predicate === "document_financial_year");
        const url = urlClaim?.value.type === "document_ref" ? httpUrl(urlClaim.value.data) : null;
        const period = [quarter, year].map((claim) => claim ? claimValueText(claim.value) : null).filter(Boolean).join(" · ");
        return (
          <div key={document.entity_id}>
            <div>
              <strong>{document.label ?? "Filed document"}</strong>
              {period && <span>{period}</span>}
            </div>
            {url && <a href={url} target="_blank" rel="noreferrer">Open</a>}
            <EvidenceButton claims={claims} onOpen={onOpen} />
          </div>
        );
      })}
    </div>
  );
}

function EvidenceDrawer({
  claims,
  evidence,
  onClose,
}: {
  claims: ReraEvidenceClaim[];
  evidence: ReraEvidenceProjection;
  onClose: () => void;
}) {
  const drawerRef = useRef<HTMLElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    closeButtonRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;

      const focusable = [...(drawerRef.current?.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [])];
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable.at(-1)!;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
      previouslyFocused?.focus();
    };
  }, [onClose]);
  const sourceByCapture = new Map(evidence.source_index.map((source) => [`${source.receipt_id}:${source.capture_id}`, source]));
  return (
    <div className="rera-drawer-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.currentTarget === event.target) onClose();
    }}>
      <aside ref={drawerRef} className="rera-drawer" role="dialog" aria-modal="true" aria-labelledby="rera-drawer-title">
        <header>
          <h2 id="rera-drawer-title">Filed evidence</h2>
          <button ref={closeButtonRef} type="button" onClick={onClose} aria-label="Close evidence">Close</button>
        </header>
        <div className="rera-drawer__body">
          {claims.map((claim) => (
            <section key={claim.claim_id}>
              <strong>{claimValueText(claim.value)}</strong>
              <span>{assertionLabel(claim.assertion_mode)}</span>
              {claim.effective_time?.start && <time dateTime={claim.effective_time.start}>{formatReraDate(claim.effective_time.start)}</time>}
              {claim.evidence.map((receipt) => {
                const source = sourceByCapture.get(`${receipt.receipt_id}:${receipt.capture_id}`);
                const url = httpUrl(source?.source_url);
                return (
                  <div key={`${claim.claim_id}:${receipt.capture_id}`}>
                    <span>Captured {source ? formatReraDate(source.captured_at) : "from K-RERA"}</span>
                    {url && <a href={url} target="_blank" rel="noreferrer">Source</a>}
                  </div>
                );
              })}
            </section>
          ))}
        </div>
      </aside>
    </div>
  );
}

function ReportSection({
  section,
  evidence,
  plans,
  onOpen,
}: {
  section: ReraReportSurfaceSection;
  evidence: ReraEvidenceProjection;
  plans: PropertyDetailResponse["plans"];
  onOpen: (claims: ReraEvidenceClaim[]) => void;
}) {
  if (section.renderer === "plans") {
    return (
      <PlanGallery
        plans={plans}
        title={section.title}
        allowedKinds={section.preview_kinds}
        maxItems={section.items_per_page}
        className="rera-section"
      />
    );
  }
  let content;
  if (section.renderer === "timeline") content = <Timeline section={section} evidence={evidence} onOpen={onOpen} />;
  else if (section.renderer === "series") content = <QuarterlySeries section={section} evidence={evidence} onOpen={onOpen} />;
  else if (section.renderer === "table") content = <InventoryTable section={section} evidence={evidence} onOpen={onOpen} />;
  else if (section.renderer === "documents") content = <Documents evidence={evidence} onOpen={onOpen} />;
  else content = <FactList section={section} evidence={evidence} onOpen={onOpen} />;
  if (!content) return null;
  return (
    <section className="rera-section" id={`rera-${section.id}`}>
      <h2>{section.title}</h2>
      {content}
    </section>
  );
}

function ReraReportPageContent({ id }: { id: string }) {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [drawerClaims, setDrawerClaims] = useState<ReraEvidenceClaim[]>([]);

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

  const visibleSections = useMemo(() => {
    if (state.status !== "ready") return [];
    return state.report.surface.sections.filter((section) => (
      section.renderer === "plans"
        ? Boolean(state.detail.plans)
        : sectionHasEvidence(section, state.report.evidence)
    ));
  }, [state]);

  if (state.status === "loading") return <PageState variant="loading" context="property" />;
  if (state.status === "error") return <PageState variant="error" context="property" message={state.message} />;

  const property = state.detail.property;
  const report = state.report;
  const title = property.title.trim();
  const latestCapture = report.evidence.coverage
    .map((coverage) => coverage.latest_observed_at)
    .sort()
    .at(-1);
  return (
    <main className="page-container-wide rera-report-page">
      <Helmet>
        <title>{title} RERA - OpenEstates</title>
        <meta name="description" content={`Officially filed RERA details for ${title}.`} />
      </Helmet>
      <header className="rera-hero">
        <Link to={`/property/${encodeURIComponent(id)}`}>Back to property</Link>
        <h1>{title}</h1>
        <p>
          {[property.area, property.city, latestCapture ? `Captured ${formatReraDate(latestCapture)}` : null]
            .filter(Boolean).join(" · ")}
        </p>
        {report.availability === "partial" && <span>Some filed sections are not available.</span>}
      </header>

      {report.availability === "unavailable" ? (
        <section className="rera-empty">
          <h2>RERA record unavailable</h2>
          <p>No matched filing is available for this property yet.</p>
        </section>
      ) : (
        visibleSections.map((section) => (
          <ReportSection
            key={section.id}
            section={section}
            evidence={report.evidence}
            plans={state.detail.plans}
            onOpen={setDrawerClaims}
          />
        ))
      )}

      {drawerClaims.length > 0 && (
        <EvidenceDrawer
          claims={drawerClaims}
          evidence={report.evidence}
          onClose={() => setDrawerClaims([])}
        />
      )}
    </main>
  );
}

export function ReraReportPage() {
  const { id = "" } = useParams();
  return <ReraReportPageContent key={id} id={id} />;
}
