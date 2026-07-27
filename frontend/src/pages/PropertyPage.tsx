import { useEffect, useState } from "react";
import { useParams, useNavigate, useSearchParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type {
  PropertyDetailResponse,
  RecommendationResponse,
  RecommendationStatus,
  SurfaceSceneResponse,
} from "../lib/types.ts";
import {
  getProperty,
  getPropertyRecommendations,
  getPropertySurface,
  parseProofFocusParam,
} from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { TrustBadge } from "../components/TrustBadge.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";
import { EvidenceStack } from "../components/evidence/EvidenceStack.tsx";
import { LivabilityBriefCard } from "../components/evidence/LivabilityBriefCard.tsx";
import { ApproachRoadTrail, hasApproachRoadTrail } from "../components/evidence/ApproachRoadTrail.tsx";
import { MarketTrendTile, hasMarketTrend } from "../components/evidence/MarketTrailBands.tsx";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { PropertySceneCard } from "../components/property/PropertySceneCard.tsx";
import { BuilderHealthPanel } from "../components/property/BuilderHealthPanel.tsx";
import { AlternativePaths } from "../components/recommendations/AlternativePaths.tsx";
import {
  detailEvidenceExcludeKindsForPlate,
  isRedundantHomeState,
} from "../lib/property-signals.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";

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
  const [searchParams] = useSearchParams();

  if (!id) return <PageState variant="not_found" context="property" message="Property was not found." />;

  const focusParam = searchParams.get("focus");
  return <PropertyPageBody key={`${id}:${focusParam ?? ""}`} id={id} focusParam={focusParam} />;
}

function PropertyPageBody({
  id,
  focusParam,
}: {
  id: string;
  focusParam: string | null;
}) {
  const navigate = useNavigate();
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [recommendations, setRecommendations] = useState<RecommendationResponse | null>(null);
  const [aroundThisHomeScene, setAroundThisHomeScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [recommendationStatus, setRecommendationStatus] =
    useState<RecommendationStatus>("pending");
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");

  useEffect(() => {
    let cancelled = false;

    getProperty(id)
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setStatus("ok");
      })
      .catch((err: Error) => {
        if (cancelled) return;
        setStatus(err.message.includes("404") ? "not_found" : "error");
      });

    return () => {
      cancelled = true;
    };
  }, [id]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertyRecommendations(propertyId)
      .then((response) => {
        if (cancelled) return;
        setRecommendations(response);
        setRecommendationStatus(response.status);
      })
      .catch(() => {
        if (cancelled) return;
        setRecommendations(null);
        setRecommendationStatus("unavailable");
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    const focus = parseProofFocusParam(focusParam);
    getPropertySurface(propertyId, "around_this_home", focus)
      .then((scene) => {
        if (!cancelled) setAroundThisHomeScene(scene);
      })
      .catch(() => {
        if (!cancelled) setAroundThisHomeScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id, focusParam]);

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

  const { property: p, society } = data;

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
  const detailEvidenceSections = data.evidence?.sections ?? [];
  const showApproachTrail = hasApproachRoadTrail(detailEvidenceSections);
  const showMarketTrend = hasMarketTrend(detailEvidenceSections);
  const aroundThisHomeContext =
    propertyMapContextFromSurfaceScene(aroundThisHomeScene, data.map_context);
  const showNearbyPlate = hasAroundThisHomePlate(aroundThisHomeContext);
  const evidenceExcludeKinds = [
    ...detailEvidenceExcludeKindsForPlate({
      showNearbyPlate,
      hasWaterOnPlate: Boolean(showNearbyPlate && aroundThisHomeContext?.water),
    }),
    ...(showMarketTrend ? ["market"] : []),
  ];
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
  const recommendationBranches = recommendations?.items ?? data.recommendation_branches ?? [];
  const recommendationRuntimeLabel = [
    recommendations?.engine_version ?? data.recommendations?.engine_version,
    recommendations?.serving_bundle_version ?? data.recommendations?.serving_bundle_version,
    recommendations?.scoring_policy_version ?? data.recommendations?.scoring_policy_version,
  ].filter(Boolean).join(" · ");

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
            navigate("/");
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
          <aside className="property-sticky-facts" aria-label="Property snapshot">
            <div className="property-sticky-facts__identity">
              <strong>{society?.name || p.title}</strong>
              <span>{p.area}</span>
            </div>
            <dl>
              <div>
                <dt>Price</dt>
                <dd>₹{formatPrice(p.price)}</dd>
              </div>
              {sizeLabel && (
                <div>
                  <dt>Carpet</dt>
                  <dd>{sizeLabel}</dd>
                </div>
              )}
              <div>
                <dt>Home</dt>
                <dd>{p.bhk} BHK</dd>
              </div>
              {(data.home_state_display || data.project_status_display) && (
                <div className="property-sticky-facts__status">
                  <dt>Status</dt>
                  <dd>{data.home_state_display || data.project_status_display}</dd>
                </div>
              )}
            </dl>
          </aside>

          {showNearbyPlate && aroundThisHomeContext && (
            <AroundThisHomePlate context={aroundThisHomeContext} />
          )}

          {showApproachTrail && (
            <ApproachRoadTrail sections={detailEvidenceSections} />
          )}

          {showLivabilityBrief && data.livability_brief && (
            <LivabilityBriefCard brief={data.livability_brief} />
          )}

          {showMarketTrend && (
            <MarketTrendTile sections={detailEvidenceSections} />
          )}

          <EvidenceStack
            key={id}
            evidence={data.evidence}
            rera={data.rera}
            googleReviews={data.external_reviews}
            excludeKinds={evidenceExcludeKinds}
          />

          <BuilderHealthPanel portfolio={data.builder_portfolio} />

          {(recommendationStatus === "pending"
            || recommendationBranches.length
            || data.similar_properties.length) ? (
            <AlternativePaths
              branches={recommendationBranches}
              nearby={data.similar_properties}
              status={recommendationStatus}
              runtimeLabel={recommendationRuntimeLabel || undefined}
            />
          ) : null}
        </main>
      </div>
    </div>
  );
}
