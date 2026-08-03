import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { getProperty, getPropertyRera } from "../lib/api.ts";
import type {
  PropertyDetailResponse,
  ReraDossier,
  ReraReportFact,
  ReraReportSection,
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
  const isCompact = !isLong && fact.value.length <= 18 && fact.label.length <= 28;
  const densityClass = isLong ? "is-long" : isCompact ? "is-compact" : "is-medium";
  return (
    <div className={`rera-report-fact ${toneClass(fact.tone)} ${densityClass}`.trim()}>
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
    <section className="rera-report-section rera-report-location-section">
      <div className="rera-report-section__head">
        <h2>{section.title}</h2>
      </div>
      {(address || coordinatesDisplay) && (
        <div className="rera-report-location">
          <div className="rera-report-location__row">
            <div className="rera-report-location__copy">
              {address && <p className="rera-report-location__address">{address.value}</p>}
              {coordinatesDisplay && (
                <p className="rera-report-location__coords">{coordinatesDisplay}</p>
              )}
            </div>
            {address && (
              <NotebookPinButton
                propertyId={propertyId}
                catalogKey={`rera-report:${propertyId}:${section.id}:${address.key}:${address.value}`}
                title={`${address.label}: ${address.value}`}
                source="RERA"
                labels={safeLabels(address.labels, address.key)}
                className="rera-report-pin"
              />
            )}
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
      {otherFacts.length > 0 && (
        <div className="rera-report-facts">
          {otherFacts.map((fact) => (
            <FactLine
              key={`${section.id}-${fact.key}-${fact.value}`}
              fact={fact}
              propertyId={propertyId}
              sectionId={section.id}
            />
          ))}
        </div>
      )}
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
  const title = displayName(property.title);
  const sourceUrl = httpUrl(currentState.dossier.source.portal_url);
  const registrationNumber = knownText(currentState.dossier.source.registration_number);
  const pageTitle = `${title} RERA - OpenEstates`;
  const locationSection = sections.find((section) => section.id === "location") ?? null;
  const otherSections = sections.filter((section) => section.id !== "location");

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
          {currentState.dossier.source.status && <span>{currentState.dossier.source.status}</span>}
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

      {locationSection && (
        <LocationSection section={locationSection} propertyId={property.id} />
      )}

      <DocumentSectionList
        sections={currentState.dossier.document_sections ?? []}
        propertyId={property.id}
      />

      {otherSections.length > 0 ? (
        otherSections.map((section) => (
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
      ) : !locationSection ? (
        <section className="rera-report-section">
          <h2>Facts</h2>
          <p className="rera-report-empty">No RERA facts are available for this home yet.</p>
        </section>
      ) : null}
    </div>
  );
}
