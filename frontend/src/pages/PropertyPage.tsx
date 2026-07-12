import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { BuilderPortfolio, PropertyDetailResponse, SourceItem, SourcePanel } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { isOnSheet, toggleSheetItem } from "../lib/sheet-store.ts";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  return !!value && value.trim().length > 0 && value !== "Not specified";
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

type SourcePanelKind = "rera" | "market" | "area" | "community" | "reviews";
type SourceTone = "good" | "watch" | "risk" | "neutral";

function clamp(value: number, min = 0, max = 1): number {
  return Math.min(max, Math.max(min, value));
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function normalizedDelta(value: number | null | undefined): number | null {
  if (value == null || !Number.isFinite(value)) return null;
  return Math.abs(value) <= 1 ? value * 100 : value;
}

function formatMedianDelta(value: number | null): string {
  if (value === null) return "No local benchmark";
  const rounded = Math.round(Math.abs(value));
  if (rounded === 0) return "At local median";
  return value < 0 ? `${rounded}% below local median` : `${rounded}% above local median`;
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
    queueMicrotask(() => setSaved(isOnSheet(id)));
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

  const { property: p, society, area, market_activity } = data;
  const pvm = market_activity.price_vs_median;

  const handleSave = () => {
    if (!id) return;
    toggleSheetItem(id);
    setSaved(!saved);
  };

  const pageTitle = `${p.title} — ${p.bhk} BHK in ${p.area} | OpenEstates`;
  const pricePerSqftLabel = hasKnownNumber(p.price_per_sqft)
    ? `${p.price_per_sqft.toLocaleString("en-IN")} /sqft`
    : null;
  const areaMedianLabel = area && hasKnownNumber(area.median_price_per_sqft)
    ? `${area.name} median ₹${area.median_price_per_sqft.toLocaleString("en-IN")}/sqft`
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
  const medianDelta = normalizedDelta(pvm?.pct_diff);
  const trust = trustPercent(data);
  const trustLabel = data.confidence_score?.label ?? (trust >= 75 ? "High" : trust >= 55 ? "Medium" : "Low");
  const risks = riskSignalsFor(p);
  const topRisk = risks[0];
  const sourceLabel = data.root_source === "rera" ? "RERA file" : data.root_source === "seller" ? "Seller file" : "Source pending";
  const sourcePanels = data.source_panels ?? [];
  const sourceFactCount = sourcePanels.reduce((sum, panel) => sum + panel.items.length, 0);
  const sourceTypes = data.data_freshness?.source_breakdown
    ? Object.entries(data.data_freshness.source_breakdown)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([source]) => source)
        .join(", ")
    : sourceLabel;
  const marketRows = [
    market_activity.interest_label,
    market_activity.saves_last_7d != null ? `${market_activity.saves_last_7d} saves this week` : null,
    market_activity.offers_last_7d != null && market_activity.offers_last_7d > 0
      ? `${market_activity.offers_last_7d} offer${market_activity.offers_last_7d > 1 ? "s" : ""} this week`
      : null,
    `Listed ${market_activity.days_on_market}d ago`,
  ].filter((row): row is string => row !== null);

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
        Back to results
      </button>

      <section className="property-brief-hero">
        <div className="property-brief-media">
          <ImageWithFallback
            src={p.hero_image}
            alt={p.title}
            className="property-brief-image"
            loading="eager"
          />
          <div className="property-brief-media-strip">
            <span>{p.area}</span>
            {hasKnownNumber(p.metro_distance_mins) && <span>{p.metro_distance_mins} min metro</span>}
            <span>{sourceLabel}</span>
          </div>
        </div>

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
          >
            {saved ? "\u2665 Saved" : "\u2661 Save"}
          </button>
        </div>
      </section>

      <div className="property-decision-layout">
        <main className="property-decision-main">
          <section className="property-decision-card property-decision-card--lead">
            <div className="property-section-heading">
              <span>At a glance</span>
              <h2>Current read</h2>
            </div>

            <div className="property-decision-metrics">
              <DecisionMetric
                label="Value"
                value={formatMedianDelta(medianDelta)}
                detail={areaMedianLabel ?? "Area benchmark unavailable"}
                tone={medianDelta !== null && medianDelta <= 0 ? "good" : medianDelta !== null && medianDelta > 8 ? "watch" : "neutral"}
              />
              <DecisionMetric
                label="Trust"
                value={trustLabel}
                detail={`${trust}/100 · ${sourceLabel}`}
                tone={trust >= 75 ? "good" : trust >= 55 ? "watch" : "risk"}
              />
              <DecisionMetric
                label="Risk"
                value={riskLabel(topRisk.value)}
                detail={`${topRisk.label} is the highest signal at ${pct(topRisk.value)}`}
                tone={topRisk.value <= 0.24 ? "good" : topRisk.value <= 0.55 ? "watch" : "risk"}
              />
              <DecisionMetric
                label="Sources"
                value={sourceFactCount > 0 ? `${sourceFactCount} facts` : "Sparse"}
                detail={sourceTypes || "Source mix not available"}
                tone={sourceFactCount >= 10 ? "good" : sourceFactCount >= 4 ? "watch" : "risk"}
              />
            </div>

          </section>

          <section className="property-evidence-section">
            <div className="property-section-heading">
              <span>Paper trail</span>
              <h2>Records behind this listing</h2>
            </div>

            <div className="property-evidence-grid">
              <div className="property-evidence-card">
                <h3>Price and market</h3>
                <EvidenceRow label="Ask" value={formatPrice(p.price)} detail={pricePerSqftLabel ?? "Rate per sqft not available"} />
                <EvidenceRow label="Benchmark" value={formatMedianDelta(medianDelta)} detail={areaMedianLabel ?? "No area benchmark"} />
                <EvidenceRow label="Demand" value={market_activity.interest_label} detail={`${market_activity.days_on_market} days on market`} />
              </div>

              <div className="property-evidence-card">
                <h3>RERA file</h3>
                <EvidenceRow label="Status" value={data.rera?.registered ? data.rera.status ?? "Registered" : "Not linked yet"} detail={data.rera?.registration_number ?? "Registration number not available"} />
                <EvidenceRow label="Timeline" value={data.rera?.completion_date ?? "Completion not available"} detail={data.rera?.delay_months ? `${data.rera.delay_months} month delay against original date` : data.rera?.original_completion_date ? `Original date: ${data.rera.original_completion_date}` : "Original date not available"} />
                <EvidenceRow label="Complaints" value={data.rera?.complaints_count != null ? String(data.rera.complaints_count) : "Not available"} detail={data.rera?.complaints_resolved_pct != null ? `${Math.round(data.rera.complaints_resolved_pct)}% resolved in file` : "Resolution data not available"} />
                <EvidenceRow
                  label="Documents"
                  value={`${Math.round(p.document_completeness_score * 100)}% complete`}
                  detail={data.rera ? "RERA file is linked; seller-level documents still need review." : "Ask for sale deed, khata, OC/CC, and dues before token."}
                />
                {data.rera?.rera_portal_url && (
                  <a className="property-text-link" href={data.rera.rera_portal_url} target="_blank" rel="noreferrer">
                    Open RERA source
                  </a>
                )}
                {data.builder_trust?.delivery_display && (
                  <BuilderTrustBadge
                    deliveryDisplay={data.builder_trust.delivery_display}
                    deliveryRate={data.builder_trust.delivery_rate}
                  />
                )}
              </div>

              <div className="property-evidence-card">
                <h3>Home facts</h3>
                <EvidenceRow
                  label="Configuration"
                  value={`${p.bhk} BHK`}
                  detail={[
                    hasKnownNumber(p.carpet_area_sqft) ? `${p.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet` : null,
                    hasKnownNumber(p.super_builtup_sqft) ? `${p.super_builtup_sqft.toLocaleString("en-IN")} sqft SBA` : null,
                  ].filter(Boolean).join(" · ") || "Size not available"}
                />
                <EvidenceRow
                  label="Floor"
                  value={hasKnownNumber(p.floor) && hasKnownNumber(p.total_floors) ? `${p.floor} of ${p.total_floors}` : "Not available"}
                  detail={isKnownText(p.facing) ? `${p.facing} facing` : "Facing not available"}
                />
                <EvidenceRow
                  label="Commute proxy"
                  value={hasKnownNumber(p.metro_distance_mins) ? `${p.metro_distance_mins} min to metro` : "Not available"}
                  detail={hasKnownNumber(p.maintenance_cost_monthly) ? `Maintenance ₹${p.maintenance_cost_monthly.toLocaleString("en-IN")}/mo` : "Maintenance not available"}
                />
              </div>
            </div>

            {data.builder_portfolio && (
              <BuilderRecordPanel portfolio={data.builder_portfolio} />
            )}
          </section>

          <SourcePanelsSection panels={sourcePanels} />

          {(society || area) && (
            <section className="property-context-panel">
              <div className="property-section-heading">
                <span>Local context</span>
                <h2>Neighbourhood and society</h2>
              </div>

              <div className="property-context-grid">
                {society && (
                  <div>
                    <h3>{society.name}</h3>
                    {(society.review_summary || society.summary) && (
                      <p>{society.review_summary || society.summary}</p>
                    )}
                    <div className="property-context-pills">
                      {isKnownText(society.builder_name) && <span>Builder: {society.builder_name}</span>}
                      {hasKnownNumber(society.year_built) && <span>{society.year_built}</span>}
                      {isKnownText(society.maintenance_sentiment) && <span>{society.maintenance_sentiment}</span>}
                    </div>
                    <div className="property-context-lists">
                      <CompactList title="Resident positives" items={society.common_positives.slice(0, 3)} tone="good" />
                      <CompactList title="Concerns" items={society.common_complaints.slice(0, 3)} tone="watch" />
                    </div>
                  </div>
                )}

                {area && (
                  <div>
                    <h3>{area.name}</h3>
                    {(area.trend_summary || area.livability_summary) && (
                      <p>{area.trend_summary || area.livability_summary}</p>
                    )}
                    <div className="property-context-pills">
                      {hasKnownNumber(area.median_price_per_sqft) && (
                        <span>₹{area.median_price_per_sqft.toLocaleString("en-IN")} /sqft median</span>
                      )}
                      {isKnownText(area.trend_direction) && <span>{area.trend_direction}</span>}
                    </div>
                    <ul className="property-context-notes">
                      {[area.metro_access_summary, area.traffic_summary, area.waterlogging_summary]
                        .filter((item): item is string => Boolean(item))
                        .slice(0, 3)
                        .map((item) => <li key={item}>{item}</li>)}
                    </ul>
                  </div>
                )}
              </div>
            </section>
          )}

          {data.similar_properties.length > 0 && (
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
          )}

        </main>

        <aside className="property-action-rail">
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

function DecisionMetric({
  label,
  value,
  detail,
  tone,
}: {
  label: string;
  value: string;
  detail: string;
  tone: "good" | "watch" | "risk" | "neutral";
}) {
  return (
    <div className={`property-decision-metric property-decision-metric--${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </div>
  );
}

function EvidenceRow({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="property-evidence-row">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </div>
  );
}

const SOURCE_PANEL_COPY: Record<SourcePanelKind, { title: string; subtitle: string }> = {
  rera: {
    title: "RERA file",
    subtitle: "Official registration, delivery dates, and file-level complaints.",
  },
  market: {
    title: "Market trail",
    subtitle: "Price bands, rate checks, and nearby comparison points.",
  },
  area: {
    title: "Area trail",
    subtitle: "Daily-life signals around access, traffic, flooding, and schools.",
  },
  community: {
    title: "Community pulse",
    subtitle: "What resident chatter keeps repeating around this project.",
  },
  reviews: {
    title: "Google reviews",
    subtitle: "Public review patterns that show up before and after visits.",
  },
};

function normalizeSourceToken(value: string | undefined): string {
  return (value ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function sourcePanelKind(panel: SourcePanel): SourcePanelKind {
  const explicitKind = normalizeSourceToken(panel.kind);
  if (
    explicitKind === "rera" ||
    explicitKind === "market" ||
    explicitKind === "area" ||
    explicitKind === "community" ||
    explicitKind === "reviews"
  ) {
    return explicitKind;
  }

  const title = `${panel.title} ${panel.subtitle}`.toLowerCase();
  if (title.includes("rera")) return "rera";
  if (title.includes("market")) return "market";
  if (title.includes("area")) return "area";
  if (title.includes("reddit") || title.includes("community") || title.includes("forum")) return "community";
  return "reviews";
}

function findSourceItem(panel: SourcePanel, ...candidates: string[]): SourceItem | undefined {
  const wanted = new Set(candidates.map(normalizeSourceToken));
  return panel.items.find((item) => (
    wanted.has(normalizeSourceToken(item.key)) ||
    wanted.has(normalizeSourceToken(item.label))
  ));
}

function cleanSourceText(item?: SourceItem | null): string {
  if (!item) return "";

  return item.value
    .replace(/^RERA Status:\s*/i, "")
    .replace(/^RERA No:\s*/i, "")
    .replace(/^Expected Completion:\s*/i, "")
    .replace(/^Resident sentiment:\s*/i, "")
    .replace(/^Google Reviews:\s*/i, "")
    .replace(/^Praised for:\s*/i, "")
    .replace(/^Criticized for:\s*/i, "")
    .replace(/^Review themes:\s*/i, "")
    .replace(/^Complaints:\s*/i, "")
    .replace(/^Top schools:\s*/i, "")
    .replace(/^Traffic:\s*/i, "")
    .replace(/^3BHK:\s*/i, "")
    .replace(/^Similar:\s*/i, "")
    .replace(/^Resident says:\s*/i, "")
    .replace(/\s+/g, " ")
    .trim();
}

function sourceItemList(item?: SourceItem | null): string[] {
  if (!item) return [];

  if (item.values && item.values.length > 0) {
    return item.values
      .map((value) => value.trim())
      .filter((value) => value.length > 0);
  }

  const value = cleanSourceText(item);
  if (!value) return [];

  const key = normalizeSourceToken(item.key || item.label);
  const splitKeys = new Set([
    "common_positives",
    "common_complaints",
    "google_top_positives",
    "google_top_negatives",
    "google_common_themes",
    "comparable_projects",
    "school_quality",
  ]);

  if (!splitKeys.has(key)) return [value];

  return value
    .split(/\s*,\s*/g)
    .map((entry) => entry.replace(/^["“]|["”]$/g, "").trim())
    .filter((entry) => entry.length > 0);
}

function sourceQuoteText(item?: SourceItem | null): string {
  return cleanSourceText(item).replace(/^["“]|["”]$/g, "").trim();
}

function sourcePanelUrl(panel: SourcePanel): string | undefined {
  return panel.items.find((item) => item.source_url)?.source_url;
}

function previewText(value: string, limit = 44): string {
  if (value.length <= limit) return value;
  return `${value.slice(0, limit - 1).trimEnd()}…`;
}

function compactPreview(parts: string[]): string {
  return parts
    .map((part) => part.trim())
    .filter((part) => part.length > 0)
    .slice(0, 3)
    .map((part, index) => previewText(part, index === 0 ? 44 : 34))
    .join(" · ");
}

function finalizeSourcePreview(panel: SourcePanel, preview: string): string {
  if (preview) return preview;
  return panel.missing[0] ? previewText(panel.missing[0], 54) : "";
}

function sourceTone(text: string): SourceTone {
  const value = text.toLowerCase();

  if (
    value.includes("0 complaint") ||
    value.includes("0 revocation") ||
    value.includes("approved") ||
    value.includes("positive") ||
    value.includes("praised") ||
    value.includes("registered")
  ) {
    return "good";
  }

  if (
    value.includes("negative") ||
    value.includes("complaint") ||
    value.includes("concern") ||
    value.includes("delay") ||
    value.includes("revocation") ||
    value.includes("waterlogging") ||
    value.includes("severe traffic")
  ) {
    return "risk";
  }

  if (
    value.includes("mixed") ||
    value.includes("construction") ||
    value.includes("under review") ||
    value.includes("unpredictable")
  ) {
    return "watch";
  }

  return "neutral";
}

function sourcePanelPreview(panel: SourcePanel, kind: SourcePanelKind): string {
  if (kind === "rera") {
    return finalizeSourcePreview(panel, compactPreview([
      cleanSourceText(findSourceItem(panel, "rera_status", "status")),
      cleanSourceText(findSourceItem(panel, "rera_completion_date", "completion")),
      cleanSourceText(findSourceItem(panel, "rera_complaints_count", "complaints"))
        || cleanSourceText(findSourceItem(panel, "rera_delay_months", "delay")),
    ]));
  }

  if (kind === "market") {
    return finalizeSourcePreview(panel, compactPreview([
      cleanSourceText(findSourceItem(panel, "price_per_sqft", "market_rate", "market rate")),
      cleanSourceText(findSourceItem(panel, "price_appreciation", "price movement")),
      cleanSourceText(findSourceItem(panel, "pricing_3bhk", "3bhk pricing")),
    ]));
  }

  if (kind === "area") {
    const parts = [
      findSourceItem(panel, "metro_details", "metro access") ? "Metro access" : "",
      findSourceItem(panel, "traffic_reality", "traffic") ? "Peak-hour traffic" : "",
      findSourceItem(panel, "waterlogging_detail", "waterlogging") ? "Rain watch" : "",
      findSourceItem(panel, "school_quality", "schools") ? "School cluster" : "",
    ];
    return finalizeSourcePreview(panel, compactPreview(parts));
  }

  if (kind === "community") {
    return finalizeSourcePreview(panel, compactPreview([
      cleanSourceText(findSourceItem(panel, "resident_sentiment", "overall take")),
      sourceItemList(findSourceItem(panel, "common_complaints", "repeated concerns"))[0] ?? "",
    ]));
  }

  return finalizeSourcePreview(panel, compactPreview([
    sourceItemList(findSourceItem(panel, "google_top_positives", "praised for"))[0] ?? "",
    sourceItemList(findSourceItem(panel, "google_top_negatives", "recurring complaints"))[0] ?? "",
  ]));
}

function SourcePanelsSection({ panels }: { panels: SourcePanel[] }) {
  if (panels.length === 0) return null;

  return (
    <section className="property-source-section">
      <div className="property-section-heading">
        <span>Source trail</span>
        <h2>What people and records say</h2>
      </div>

      <div className="source-panel-grid">
        {panels.map((panel, index) => {
          const kind = sourcePanelKind(panel);
          const copy = SOURCE_PANEL_COPY[kind];
          const preview = sourcePanelPreview(panel, kind);

          return (
            <details key={`${panel.title}-${kind}`} className={`source-panel source-panel--${kind}`} open={index === 0}>
              <summary>
                <i className="source-chevron" aria-hidden="true" />
                <div className="source-panel-headline">
                  <h3>{copy.title}</h3>
                  <p>{copy.subtitle}</p>
                </div>
                {preview && <span className="source-panel-glance">{preview}</span>}
              </summary>

              <div className="source-panel-body">
                <SourcePanelBody panel={panel} kind={kind} />
                <SourceMissingNotes items={panel.missing} />
              </div>
            </details>
          );
        })}
      </div>
    </section>
  );
}

function SourcePanelBody({ panel, kind }: { panel: SourcePanel; kind: SourcePanelKind }) {
  if (kind === "rera") {
    const status = findSourceItem(panel, "rera_status", "status");
    const registration = findSourceItem(panel, "rera_number", "registration");
    const completion = findSourceItem(panel, "rera_completion_date", "completion");
    const delay = findSourceItem(panel, "rera_delay_months", "delay");
    const complaints = findSourceItem(panel, "rera_complaints_count", "complaints");
    const revocations = findSourceItem(panel, "rera_builder_revocations", "builder revocations");
    const sourceUrl = sourcePanelUrl(panel);

    const stats = [
      status ? { label: "Status", value: cleanSourceText(status), tone: sourceTone(cleanSourceText(status)) } : null,
      registration ? { label: "Registration", value: cleanSourceText(registration), tone: "neutral" as SourceTone } : null,
      completion ? { label: "Completion", value: cleanSourceText(completion), tone: "neutral" as SourceTone } : null,
      delay ? { label: "Delay", value: cleanSourceText(delay), tone: "watch" as SourceTone } : null,
      complaints ? { label: "Complaints", value: cleanSourceText(complaints), tone: sourceTone(cleanSourceText(complaints)) } : null,
      revocations ? { label: "Revocations", value: cleanSourceText(revocations), tone: sourceTone(cleanSourceText(revocations)) } : null,
    ].filter((stat): stat is { label: string; value: string; tone: SourceTone } => stat !== null);

    return (
      <div className="source-panel-stack">
        <div className="source-stat-grid">
          {stats.map((stat) => (
            <SourceStat key={stat.label} label={stat.label} value={stat.value} tone={stat.tone} />
          ))}
        </div>
        {sourceUrl && (
          <a className="property-text-link source-panel-link" href={sourceUrl} target="_blank" rel="noreferrer">
            Open RERA source
          </a>
        )}
      </div>
    );
  }

  if (kind === "market") {
    const pricing = findSourceItem(panel, "pricing_3bhk", "3bhk pricing");
    const rate = findSourceItem(panel, "price_per_sqft", "market rate");
    const appreciation = findSourceItem(panel, "price_appreciation", "price movement");
    const comparables = sourceItemList(findSourceItem(panel, "comparable_projects", "nearby comparables"));

    return (
      <div className="source-panel-stack">
        <div className="source-stat-grid source-stat-grid--wide">
          {pricing && <SourceStat label="3BHK band" value={cleanSourceText(pricing)} tone="neutral" />}
          {rate && <SourceStat label="Rate check" value={cleanSourceText(rate)} tone="good" />}
          {appreciation && <SourceStat label="Cycle" value={cleanSourceText(appreciation)} tone="watch" />}
        </div>
        <SourceTagRow title="Compared against" items={comparables} />
      </div>
    );
  }

  if (kind === "area") {
    const cards = [
      (() => {
        const item = findSourceItem(panel, "metro_details", "metro access");
        return item ? { label: "Metro access", value: cleanSourceText(item), tone: "good" as SourceTone } : null;
      })(),
      (() => {
        const item = findSourceItem(panel, "traffic_reality", "traffic");
        return item ? { label: "Traffic", value: cleanSourceText(item), tone: "watch" as SourceTone } : null;
      })(),
      (() => {
        const item = findSourceItem(panel, "waterlogging_detail", "waterlogging");
        return item ? { label: "Waterlogging", value: cleanSourceText(item), tone: "risk" as SourceTone } : null;
      })(),
      (() => {
        const item = findSourceItem(panel, "school_quality", "schools");
        return item ? { label: "Schools", value: cleanSourceText(item), tone: "good" as SourceTone } : null;
      })(),
    ].filter((card): card is { label: string; value: string; tone: SourceTone } => card !== null);

    return (
      <div className="source-signal-grid">
        {cards.map((card) => (
          <SourceSignalCard key={card.label} label={card.label} value={card.value} tone={card.tone} />
        ))}
      </div>
    );
  }

  if (kind === "community") {
    const overall = cleanSourceText(findSourceItem(panel, "resident_sentiment", "overall take"));
    const summary = cleanSourceText(findSourceItem(panel, "sentiment_summary", "what forums point to"));
    const quoteItem = findSourceItem(panel, "best_quote", "quote");
    const quote = sourceQuoteText(quoteItem);
    const positives = sourceItemList(findSourceItem(panel, "common_positives", "repeated positives"));
    const concerns = sourceItemList(findSourceItem(panel, "common_complaints", "repeated concerns"));

    return (
      <div className="source-panel-stack">
        {(overall || summary) && (
          <div className="source-lead">
            {overall && (
              <span className={`source-sentiment-pill source-sentiment-pill--${sourceTone(overall)}`}>
                {overall}
              </span>
            )}
            {summary && <p>{summary}</p>}
          </div>
        )}

        {quote && (
          <blockquote className="source-quote">
            <p>{quote}</p>
            <span>{quoteItem?.source_type === "Llm" ? "Representative line" : "Quoted line"}</span>
          </blockquote>
        )}

        <div className="source-list-grid">
          <SourceListCard title="What people like" items={positives} tone="good" />
          <SourceListCard title="What people complain about" items={concerns} tone="watch" />
        </div>
      </div>
    );
  }

  const overall = cleanSourceText(findSourceItem(panel, "google_sentiment", "overall take"));
  const positives = sourceItemList(findSourceItem(panel, "google_top_positives", "praised for"));
  const concerns = sourceItemList(findSourceItem(panel, "google_top_negatives", "recurring complaints"));
  const themes = sourceItemList(findSourceItem(panel, "google_common_themes", "themes"));

  return (
    <div className="source-panel-stack">
      {overall && (
        <div className="source-lead">
          <p>{overall}</p>
        </div>
      )}
      <div className="source-list-grid">
        <SourceListCard title="Often praised" items={positives} tone="good" />
        <SourceListCard title="Often criticized" items={concerns} tone="watch" />
      </div>
      <SourceTagRow title="Themes" items={themes} />
    </div>
  );
}

function SourceStat({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: SourceTone;
}) {
  return (
    <div className={`source-stat source-stat--${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function SourceSignalCard({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: SourceTone;
}) {
  return (
    <div className={`source-signal-card source-signal-card--${tone}`}>
      <span>{label}</span>
      <p>{value}</p>
    </div>
  );
}

function SourceListCard({
  title,
  items,
  tone,
}: {
  title: string;
  items: string[];
  tone: "good" | "watch";
}) {
  if (items.length === 0) return null;

  return (
    <div className="source-list-card">
      <CompactList title={title} items={items} tone={tone} />
    </div>
  );
}

function SourceTagRow({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;

  return (
    <div className="source-tag-row">
      <span>{title}</span>
      <div className="source-chip-wrap">
        {items.map((item) => (
          <span key={item} className="source-chip">{item}</span>
        ))}
      </div>
    </div>
  );
}

function SourceMissingNotes({ items }: { items: string[] }) {
  if (items.length === 0) return null;

  return (
    <div className="source-missing-list">
      <span>Still missing</span>
      <ul>
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
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

function CompactList({ title, items, tone }: { title: string; items: string[]; tone: "good" | "watch" }) {
  if (items.length === 0) return null;

  return (
    <div className={`property-compact-list property-compact-list--${tone}`}>
      <h4>{title}</h4>
      <ul>
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
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
      <div className="property-risk-track">
        <span className={`property-risk-fill property-risk-fill--${tone}`} style={{ width: pct(signal.value) }} />
      </div>
    </div>
  );
}
