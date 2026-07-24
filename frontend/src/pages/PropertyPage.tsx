import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { BuilderPortfolio, PropertyDetailResponse } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { TrustBadge } from "../components/TrustBadge.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";
import { EvidenceStack } from "../components/evidence/EvidenceStack.tsx";
import { LivabilityBriefCard } from "../components/evidence/LivabilityBriefCard.tsx";
import { ApproachRoadTrail, hasApproachRoadTrail } from "../components/evidence/ApproachRoadTrail.tsx";
import {
  AroundThisHomePlate,
  hasAroundThisHomePlate,
} from "../components/evidence/AroundThisHomePlate.tsx";
import { PropertySceneCard } from "../components/property/PropertySceneCard.tsx";
import { AlternativePaths } from "../components/recommendations/AlternativePaths.tsx";
import {
  detailEvidenceExcludeKindsForPlate,
  isRedundantHomeState,
} from "../lib/property-signals.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  if (!value) return false;
  const lowered = value.trim().toLowerCase();
  return lowered.length > 0 && lowered !== "not specified" && lowered !== "unknown" && lowered !== "n/a";
}

function compactLifecycleLabel(value: string): string {
  const normalized = value
    .replace(/^home state:\s*/i, "")
    .replace(/_/g, " ")
    .trim();
  if (!normalized) return value;
  return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}

function buildPropertyJsonLd(p: PropertyDetailResponse["property"]) {
  const sizeDescription = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft} sqft`
    : "available configuration";
  const jsonLd: Record<string, unknown> = {
    "@context": "https://schema.org",
    "@type": "RealEstateListing",
    name: p.title,
    description: p.description_summary || `${p.bhk} BHK, ${sizeDescription} in ${p.area}, ${p.city}`,
    url: `https://openestates.in/property/${p.id}`,
    offers: {
      "@type": "Offer",
      price: p.price,
      priceCurrency: "INR",
    },
    address: {
      "@type": "PostalAddress",
      addressLocality: p.area,
      addressRegion: p.city,
    },
    numberOfRooms: p.bhk,
  };
  if (hasKnownNumber(p.carpet_area_sqft)) {
    jsonLd.floorSize = {
      "@type": "QuantitativeValue",
      value: p.carpet_area_sqft,
      unitCode: "FTK",
    };
  }
  if (p.hero_image) {
    jsonLd.image = p.hero_image;
  }
  return jsonLd;
}

export function PropertyPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");

  useEffect(() => {
    if (!id) return;
    setData(null);
    setStatus("loading");

    getProperty(id)
      .then((d) => {
        setData(d);
        setStatus("ok");
      })
      .catch((err: Error) => {
        setStatus(err.message.includes("404") ? "not_found" : "error");
      });
  }, [id]);

  if (status === "loading") return (
    <div className="page-container-wide">
      {/* Hero placeholder */}
      <div className="skeleton-bar" style={{ width: "100%", height: "320px", borderRadius: "var(--radius-md)", marginBottom: "1.5rem" }} />
      {/* Title bar */}
      <div className="skeleton-bar" style={{ width: "60%", height: "28px", marginBottom: "0.5rem" }} />
      {/* Subtitle */}
      <div className="skeleton-bar" style={{ width: "40%", height: "16px", marginBottom: "1rem" }} />
      {/* Price bar */}
      <div className="skeleton-bar" style={{ width: "25%", height: "24px", marginBottom: "0.75rem" }} />
      {/* Tags row */}
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "2rem" }}>
        <div className="skeleton-bar" style={{ width: "60px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "80px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "70px", height: "24px", borderRadius: "999px" }} />
        <div className="skeleton-bar" style={{ width: "90px", height: "24px", borderRadius: "999px" }} />
      </div>
      {/* Two-column layout */}
      <div className="property-layout">
        <div className="property-main">
          <div className="skeleton-detail-section skeleton-bar" />
          <div className="skeleton-detail-section skeleton-bar" />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "140px" }} />
        </div>
        <div className="property-sidebar">
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "120px" }} />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "160px" }} />
        </div>
      </div>
    </div>
  );
  if (status === "not_found") return <PageState variant="not_found" context="property" message={`Property "${id}" was not found.`} />;
  if (status === "error") return <PageState variant="error" context="property" />;
  if (!data) return null;

  const { property: p, society, market_activity } = data;

  const pageTitle = `${p.title} — ${p.bhk} BHK in ${p.area} | OpenEstates`;
  const pricePerSqftLabel = hasKnownNumber(p.price_per_sqft)
    ? `${p.price_per_sqft.toLocaleString("en-IN")} /sqft`
    : null;
  const sizeLabel = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft.toLocaleString("en-IN")} sqft`
    : null;
  const pageDescription = [
    `${p.bhk} BHK`,
    sizeLabel,
    `in ${society?.name ? society.name + ", " : ""}${p.area}`,
    formatPrice(p.price),
    pricePerSqftLabel,
    `${p.area}, ${p.city}`,
  ].filter(Boolean).join(". ");
  const sourceLabel = data.root_source === "rera" ? "RERA file" : data.root_source === "seller" ? "Seller file" : "Source pending";
  const marketRows = [
    market_activity.interest_label,
    market_activity.saves_last_7d != null ? `${market_activity.saves_last_7d} saves this week` : null,
    market_activity.offers_last_7d != null && market_activity.offers_last_7d > 0
      ? `${market_activity.offers_last_7d} offer${market_activity.offers_last_7d > 1 ? "s" : ""} this week`
      : null,
    `Listed ${market_activity.days_on_market}d ago`,
  ].filter((row): row is string => row !== null);
  const detailEvidenceSections = data.evidence?.sections ?? [];
  const showApproachTrail = hasApproachRoadTrail(detailEvidenceSections);
  const showNearbyPlate = hasAroundThisHomePlate(data.map_context);
  const evidenceExcludeKinds = detailEvidenceExcludeKindsForPlate({
    showNearbyPlate,
    hasWaterOnPlate: Boolean(showNearbyPlate && data.map_context?.water),
  });
  const showHomeStateChip = Boolean(
    data.home_state_display
    && !isRedundantHomeState(
      data.home_state_display,
      data.project_status_display,
      p.possession_status,
    ),
  );
  const lifecycleTag = showHomeStateChip && data.home_state_display
    ? compactLifecycleLabel(data.home_state_display)
    : null;
  const showLivabilityBrief = Boolean(
    data.livability_brief?.summary_paragraph?.trim()
  );

  return (
    <div className="page-container-wide property-decision-page">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={pageDescription} />
        <meta property="og:title" content={pageTitle} />
        <meta property="og:description" content={pageDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
        {p.hero_image && <meta property="og:image" content={p.hero_image} />}
        <script type="application/ld+json">{JSON.stringify(buildPropertyJsonLd(p))}</script>
      </Helmet>
      <button
        onClick={() => {
          if (window.history.length > 1) {
            navigate(-1);
          } else {
            navigate("/results");
          }
        }}
        className="back-link property-brief-back"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
        Back
      </button>

      <section className="property-brief-hero">
        <PropertySceneCard
          title={p.title}
          societyName={society?.name}
          heroImage={p.hero_image}
          images={p.images}
          societyId={p.society_id}
          chips={[
            { label: "Area", value: p.area },
            ...(hasKnownNumber(p.metro_distance_mins)
              ? [{ label: "Metro", value: `${p.metro_distance_mins} min` }]
              : []),
            { label: "Source", value: sourceLabel },
          ]}
        />

        <div className="property-brief-copy">
          <h1>
            {p.title}
          </h1>
          <p className="property-brief-location">
            {society?.name ? `${society.name} · ` : ""}{p.area}, {p.city}
          </p>

          <div className="property-brief-price-row">
            <strong>{formatPrice(p.price)}</strong>
            {pricePerSqftLabel && <span>{pricePerSqftLabel}</span>}
          </div>

          <div className="property-proof-strip">
            <TrustBadge rootSource={data.root_source} compact />
            {data.builder_trust?.delivery_display && (
              <BuilderTrustBadge
                deliveryDisplay={data.builder_trust.delivery_display}
                deliveryRate={data.builder_trust.delivery_rate}
                compact
              />
            )}
            {data.rera?.registered && (
              <span className="property-proof-strip__chip property-proof-strip__chip--positive">
                RERA verified
              </span>
            )}
          </div>

          <div className="property-brief-tags">
            <span>{p.bhk} BHK</span>
            {hasKnownNumber(p.carpet_area_sqft) && (
              <span>{p.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet</span>
            )}
            {lifecycleTag && <span>{lifecycleTag}</span>}
            {isKnownText(p.facing) && <span>{p.facing} facing</span>}
            {hasKnownNumber(p.floor) && hasKnownNumber(p.total_floors) && (
              <span>Floor {p.floor}/{p.total_floors}</span>
            )}
            <ProjectStatusTag
              status={data.project_status}
              displayText={data.project_status_display}
              possessionStatus={p.possession_status}
            />
          </div>
        </div>
      </section>

      <div className="property-decision-layout">
        <main className="property-decision-main">
          {showNearbyPlate && data.map_context && (
            <AroundThisHomePlate context={data.map_context} />
          )}

          {marketRows.length > 0 && (
            <section className="property-market-strip" aria-label="Market pulse">
              <strong>Market pulse</strong>
              <div>
                {marketRows.map((row) => (
                  <span key={row}>{row}</span>
                ))}
              </div>
            </section>
          )}

          {showApproachTrail && (
            <ApproachRoadTrail sections={detailEvidenceSections} />
          )}

          {showLivabilityBrief && data.livability_brief && (
            <LivabilityBriefCard brief={data.livability_brief} />
          )}

          <EvidenceStack
            key={id}
            evidence={data.evidence}
            excludeKinds={evidenceExcludeKinds}
          />

          {data.builder_portfolio ? (
            <section className="property-evidence-section">
              <BuilderRecordPanel portfolio={data.builder_portfolio} />
            </section>
          ) : isKnownText(p.builder_name) && (
            <section className="property-evidence-section">
              <div className="builder-norecord-card">
                <div>
                  <span>Builder record</span>
                  <h3>{p.builder_name}</h3>
                </div>
                <p>No other ongoing projects tracked in RERA.</p>
              </div>
            </section>
          )}

          {(data.recommendation_branches?.length ?? 0) > 0 ? (
            <AlternativePaths branches={data.recommendation_branches ?? []} />
          ) : data.similar_properties.length > 0 ? (
            <section className="property-similar-section">
              <div className="property-section-heading">
                <span>Compared with</span>
                <h2>Nearby alternatives</h2>
              </div>
              <div className="property-similar-grid">
                {data.similar_properties.slice(0, 3).map((sp) => (
                  <Link key={sp.id} to={`/property/${sp.id}`} className="property-similar-card">
                    <ImageWithFallback
                      src={sp.hero_image || ""}
                      alt={sp.title}
                      className="property-similar-image"
                    />
                    <div>
                      <strong>{sp.title}</strong>
                      <span>{sp.society_name} · {sp.area}</span>
                      <b>{formatPrice(sp.price)}</b>
                    </div>
                  </Link>
                ))}
              </div>
            </section>
          ) : null}

        </main>
      </div>
    </div>
  );
}

function BuilderRecordPanel({ portfolio }: { portfolio: BuilderPortfolio }) {
  const revocationText = portfolio.revocations == null
    ? "Revocations not available"
    : `${portfolio.revocations} revocation${portfolio.revocations === 1 ? "" : "s"}`;

  return (
    <div className="builder-record-panel">
      <div className="builder-record-header">
        <div>
          <span>Builder record</span>
          <h3>{portfolio.builder_name}</h3>
        </div>
        <div className="builder-record-summary">
          <strong>{portfolio.rera_registered_projects}/{portfolio.tracked_projects}</strong>
          <span>tracked projects with RERA files</span>
        </div>
      </div>

      <div className="builder-record-stats">
        <div>
          <span>Delayed</span>
          <strong>{portfolio.delayed_projects}</strong>
        </div>
        <div>
          <span>Complaints</span>
          <strong>{portfolio.complaint_projects}</strong>
        </div>
        <div>
          <span>Revocations</span>
          <strong>{revocationText}</strong>
        </div>
      </div>

      <div className="builder-project-list">
        {portfolio.projects.map((project) => {
          const hasDelay = project.delay_months != null && project.delay_months > 0;
          const hasComplaints = project.complaints_count != null && project.complaints_count > 0;

          return (
            <div
              key={`${project.property_id}-${project.project_name}`}
              className={`builder-project-row ${project.current ? "builder-project-row--current" : ""}`}
            >
              <Link to={`/property/${project.property_id}`} className="builder-project-main">
                <div>
                  <strong>{project.project_name}</strong>
                  <span>{project.area}{project.current ? " · current file" : ""}</span>
                </div>
                <div>
                  <b>{project.rera_status ?? "RERA pending"}</b>
                  <span>{project.rera_number ?? project.project_status_display ?? "No registration linked"}</span>
                </div>
              </Link>
              <div className="builder-project-actions">
                <div className="builder-project-flags">
                  {hasDelay && (
                    <span>{project.delay_months} mo delay</span>
                  )}
                  {hasComplaints && (
                    <span>{project.complaints_count} complaint{project.complaints_count === 1 ? "" : "s"}</span>
                  )}
                  {!hasDelay && !hasComplaints && (
                    <span>No flags in file</span>
                  )}
                </div>
                {project.rera_portal_url && (
                  <a className="builder-project-source" href={project.rera_portal_url} target="_blank" rel="noreferrer">
                    RERA source
                  </a>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
