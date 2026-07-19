import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { BuilderPortfolio, PropertyDetailResponse } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { isSaved, toggleSaved } from "../lib/sheet-store.ts";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { TrustBadge } from "../components/TrustBadge.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";
import { EvidenceStack } from "../components/evidence/EvidenceStack.tsx";
import { ApproachRoadTrail, hasApproachRoadTrail } from "../components/evidence/ApproachRoadTrail.tsx";
import { PropertySceneCard } from "../components/property/PropertySceneCard.tsx";
import { AlternativePaths } from "../components/recommendations/AlternativePaths.tsx";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import { panelsToSections, topEvidenceGlance } from "../lib/evidence.ts";

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

type DecisionTone = "compare" | "verify" | "negotiate";

type RiskSignal = {
  label: string;
  value: number;
};

function clamp(value: number, min = 0, max = 1): number {
  return Math.min(max, Math.max(min, value));
}

function normalizedDelta(value: number | null | undefined): number | null {
  if (value == null || !Number.isFinite(value)) return null;
  return Math.abs(value) <= 1 ? value * 100 : value;
}

function riskSignalsFor(p: PropertyDetailResponse["property"]): RiskSignal[] {
  return [
    { label: "Legal", value: clamp(p.litigation_risk) },
    { label: "Waterlogging", value: clamp(p.waterlogging_risk_score) },
    { label: "Traffic", value: clamp(1 - p.traffic_score) },
    { label: "Noise", value: clamp(p.noise_score) },
  ].sort((a, b) => b.value - a.value);
}

function riskLabel(value: number): "Low" | "Moderate" | "High" {
  if (value <= 0.24) return "Low";
  if (value <= 0.55) return "Moderate";
  return "High";
}

function trustPercent(data: PropertyDetailResponse): number {
  if (data.confidence_score?.overall != null) return Math.round(data.confidence_score.overall * 100);
  return data.transparency_score.overall;
}

function buildDecision(data: PropertyDetailResponse): {
  label: string;
  tone: DecisionTone;
  summary: string;
} {
  const p = data.property;
  const delta = normalizedDelta(data.market_activity.price_vs_median?.pct_diff);
  const trust = trustPercent(data);
  const risks = riskSignalsFor(p);
  const topRisk = risks[0];
  const topRiskLabel = riskLabel(topRisk.value);

  if (trust < 60) {
    return {
      label: "Needs document review",
      tone: "verify",
      summary: data.rera?.registered
        ? "RERA is verified. Unit-level seller proof still needs review."
        : "Regulatory and seller documents are not complete enough yet.",
    };
  }

  if (topRiskLabel === "High") {
    return {
      label: "Verify risk before visit",
      tone: "verify",
      summary: `${topRisk.label} risk is the main blocker. Clear that before treating this as a finalist.`,
    };
  }

  if (delta !== null && delta > 8) {
    return {
      label: "Price needs support",
      tone: "negotiate",
      summary: "Ask is above the local benchmark; compare recent resale prices before a visit.",
    };
  }

  return {
    label: "Worth comparing",
    tone: "compare",
    summary: "Price, source, and risk are balanced enough to compare against other saved homes.",
  };
}

export function PropertyPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!id) return;
    queueMicrotask(() => setSaved(isSaved(id)));
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

  const handleSave = () => {
    if (!id) return;
    toggleSaved(id);
    setSaved(!saved);
  };

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
    "Transparency scores, risk signals, and tradeoffs.",
  ].filter(Boolean).join(". ");
  const decision = buildDecision(data);
  const risks = riskSignalsFor(p);
  const sourceLabel = data.root_source === "rera" ? "RERA file" : data.root_source === "seller" ? "Seller file" : "Source pending";
  const sourcePanels = data.source_panels ?? [];
  const marketRows = [
    market_activity.interest_label,
    market_activity.saves_last_7d != null ? `${market_activity.saves_last_7d} saves this week` : null,
    market_activity.offers_last_7d != null && market_activity.offers_last_7d > 0
      ? `${market_activity.offers_last_7d} offer${market_activity.offers_last_7d > 1 ? "s" : ""} this week`
      : null,
    `Listed ${market_activity.days_on_market}d ago`,
  ].filter((row): row is string => row !== null);
  const fallbackEvidenceSections = panelsToSections(sourcePanels);
  const detailEvidenceSections = data.evidence?.sections?.length
    ? data.evidence.sections
    : fallbackEvidenceSections;
  const showApproachTrail = hasApproachRoadTrail(detailEvidenceSections);
  const proofGlance = topEvidenceGlance(data.evidence, 1)[0] ?? null;

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
          <span className={`property-verdict property-verdict--${decision.tone}`}>
            {decision.label}
          </span>
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

          <p className="property-brief-summary">{decision.summary}</p>

          <div className="property-proof-strip">
            {data.home_state_display && (
              <span className="property-proof-strip__chip">{data.home_state_display}</span>
            )}
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
            {proofGlance && <span className="property-proof-strip__read">{proofGlance}</span>}
          </div>

          <div className="property-brief-tags">
            <span>{p.bhk} BHK</span>
            {hasKnownNumber(p.carpet_area_sqft) && (
              <span>{p.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet</span>
            )}
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

          <button
            onClick={handleSave}
            className={`btn property-hero-save ${saved ? "btn-primary" : "btn-outline"}`}
            aria-pressed={saved}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
            </svg>
            {saved ? "Saved" : "Save"}
          </button>
        </div>
      </section>

      <div className="property-decision-layout">
        <main className="property-decision-main">
          {showApproachTrail && (
            <ApproachRoadTrail sections={detailEvidenceSections} />
          )}

          <EvidenceStack
            evidence={data.evidence}
            fallbackSections={fallbackEvidenceSections}
            excludeKinds={showApproachTrail ? ["approach_road"] : []}
          />

          {data.builder_portfolio && (
            <section className="property-evidence-section">
              <BuilderRecordPanel portfolio={data.builder_portfolio} />
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

        <aside className="property-action-rail">
          <div className="property-mini-card property-plan-entry-card">
            <span>{BUY_VS_RENT.kicker}</span>
            <h3>Would buying beat renting for you?</h3>
            <p>Compare EMI, rent, and investing the difference — with a repayment timeline.</p>
            <Link to={`/property/${p.id}/plan`} className="property-plan-entry-link">
              {BUY_VS_RENT.cta}
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M5 12h14M13 6l6 6-6 6" />
              </svg>
            </Link>
          </div>
          <div className="property-mini-card property-rail-intel">
            <div>
              <h3>Risk list</h3>
              <div className="property-risk-stack">
                {risks.slice(0, 4).map((risk) => (
                  <RiskBar key={risk.label} signal={risk} />
                ))}
              </div>
            </div>
            <div>
              <h3>Market pulse</h3>
              <div className="property-market-list">
                {marketRows.map((row) => (
                  <span key={row}>{row}</span>
                ))}
              </div>
            </div>
          </div>
        </aside>
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


function RiskBar({ signal }: { signal: RiskSignal }) {
  const label = riskLabel(signal.value);
  const tone = signal.value <= 0.24 ? "good" : signal.value <= 0.55 ? "watch" : "risk";

  return (
    <div className="property-risk-row">
      <div>
        <span>{signal.label}</span>
        <strong className={`property-risk-label property-risk-label--${tone}`}>{label}</strong>
      </div>
    </div>
  );
}
