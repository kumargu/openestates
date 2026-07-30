import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import type {
  PropertyDetailResponse,
  ReraDossier,
  ReraDocumentSection,
  ReraReportFact,
  ReraReportSection,
} from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { NotebookPinButton } from "../components/notebook/NotebookPinButton.tsx";
import { LinkIcon } from "../components/evidence/EvidenceIcons.tsx";
import type { NotebookLabelId } from "../lib/notebook.ts";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; detail: PropertyDetailResponse; dossier: ReraDossier }
  | { status: "error"; message: string };

function knownText(value?: string | null): string | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  if (["unknown", "not specified", "n/a", "na", "none", "null"].includes(normalized.toLowerCase())) {
    return null;
  }
  return normalized;
}

function displayName(value: string): string {
  const keepUpper = new Set(["BHK", "ITPL", "JP", "KR"]);
  return value.replace(/\b[A-Z][A-Z0-9&.'-]*\b/g, (word) => {
    if (keepUpper.has(word) || /\d/.test(word)) return word;
    return word.charAt(0) + word.slice(1).toLowerCase();
  });
}

function httpUrl(value?: string): string | null {
  const known = knownText(value);
  if (!known) return null;
  try {
    const url = new URL(known);
    return url.protocol === "http:" || url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

function toneClass(tone?: string): string {
  if (!tone || tone === "neutral" || tone === "default") return "";
  return `is-${tone}`;
}

function kindLabel(value: string): string {
  const normalized = value.replace(/[_-]+/g, " ").trim();
  return normalized ? displayName(normalized) : "Document";
}

function safeLabels(labels: string[] | undefined, key: string): NotebookLabelId[] {
  const next = labels?.filter(Boolean) ?? [];
  if (next.length > 0) return [...new Set(next)].slice(0, 4);
  const keyText = key.toLowerCase();
  if (keyText.includes("complaint")) return ["complaints", "risk", "legal"];
  if (keyText.includes("delay") || keyText.includes("litigation")) return ["risk", "legal"];
  return ["legal"];
}

function fallbackSections(dossier: ReraDossier): ReraReportSection[] {
  if (dossier.fact_sections?.length) return dossier.fact_sections;

  const facts: ReraReportFact[] = dossier.compare_items
    .filter((item) => knownText(item.value))
    .map((item) => ({
      key: item.key,
      label: item.label,
      value: item.value,
      tone: item.tone,
      labels: safeLabels(item.labels, item.key),
      confidence: 1,
      learned_at: dossier.source.last_verified ?? "",
    }));

  return facts.length > 0 ? [{ id: "facts", title: "Facts", facts }] : [];
}

function FactLine({
  fact,
  propertyId,
  sectionId,
}: {
  fact: ReraReportFact;
  propertyId: string;
  sectionId: string;
}) {
  const isLong = fact.value.length > 70 || fact.label.length > 32;
  return (
    <div className={`rera-report-fact ${toneClass(fact.tone)} ${isLong ? "is-long" : ""}`.trim()}>
      <div className="rera-report-fact__copy">
        <span>{fact.label}</span>
        <strong>{fact.value}</strong>
      </div>
      <div className="rera-report-fact__actions">
        <NotebookPinButton
          propertyId={propertyId}
          catalogKey={`rera-report:${propertyId}:${sectionId}:${fact.key}:${fact.value}`}
          title={`${fact.label}: ${fact.value}`}
          source="RERA"
          labels={safeLabels(fact.labels, fact.key)}
          className="rera-report-pin"
        />
      </div>
    </div>
  );
}

function DocumentSectionList({
  sections,
  propertyId,
}: {
  sections: ReraDocumentSection[];
  propertyId: string;
}) {
  const visibleSections = sections
    .map((section) => ({
      ...section,
      items: section.items?.filter((item) => httpUrl(item.source_url)) ?? [],
    }))
    .filter((section) => section.items.length > 0);

  if (visibleSections.length === 0) return null;

  return (
    <section className="rera-report-section rera-report-documents">
      <div className="rera-report-section__head">
        <h2>Documents</h2>
      </div>
      <div className="rera-report-document-groups">
        {visibleSections.map((section) => (
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

export function ReraReportPage() {
  const { id } = useParams<{ id: string }>();
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    if (!id) {
      setState({ status: "error", message: "This report needs a property id." });
      return;
    }

    const controller = new AbortController();
    setState({ status: "loading" });
    Promise.all([
      getProperty(id, { signal: controller.signal }),
      getPropertyRera(id),
    ])
      .then(([detail, dossier]) => setState({ status: "ready", detail, dossier }))
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setState({
          status: "error",
          message: error instanceof Error ? error.message : "Unable to load RERA report.",
        });
      });

    return () => controller.abort();
  }, [id]);

  const sections = useMemo(() => (
    state.status === "ready" ? fallbackSections(state.dossier) : []
  ), [state]);

  if (!id) return <PageState variant="not_found" context="property" />;
  if (state.status === "loading") return <PageState variant="loading" context="property" message="Opening RERA." />;
  if (state.status === "error") return <PageState variant="error" context="property" message={state.message} />;

  const property = state.detail.property;
  const title = displayName(property.title);
  const sourceUrl = httpUrl(state.dossier.source.portal_url);
  const registrationNumber = knownText(state.dossier.source.registration_number);
  const pageTitle = `${title} RERA - OpenEstates`;

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
        <div className="rera-report-subline">
          <span>{property.area}, {property.city}</span>
          {state.dossier.source.status && <span>{state.dossier.source.status}</span>}
        </div>
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
                Open
              </a>
            )}
          </div>
        )}
      </header>

      <DocumentSectionList
        sections={state.dossier.document_sections ?? []}
        propertyId={property.id}
      />

      {sections.length > 0 ? (
        sections.map((section) => (
          <section key={section.id} className="rera-report-section">
            <div className="rera-report-section__head">
              <h2>{section.title}</h2>
            </div>
            <div className="rera-report-facts">
              {section.facts.map((fact) => (
                <FactLine
                  key={`${section.id}-${fact.key}-${fact.value}`}
                  fact={fact}
                  propertyId={property.id}
                  sectionId={section.id}
                />
              ))}
            </div>
          </section>
        ))
      ) : (
        <section className="rera-report-section">
          <h2>Facts</h2>
          <p className="rera-report-empty">No RERA facts are available for this home yet.</p>
        </section>
      )}
    </div>
  );
}
