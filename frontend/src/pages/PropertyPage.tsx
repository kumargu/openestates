import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { BuilderPortfolio, PropertyDetailResponse, SourcePanel } from "../lib/types.ts";
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

function formatSourceDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Date not available";
  return date.toLocaleDateString("en-IN", { day: "2-digit", month: "short", year: "numeric" });
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
        {panels.map((panel, index) => (
          <details key={panel.title} className="source-panel" open={index === 0}>
            <summary>
              <div>
                <h3>{panel.title}</h3>
                <p>{panel.subtitle}</p>
              </div>
              <div className="source-panel-actions">
                <strong className="source-panel-count">{panel.items.length} stored</strong>
                <span className="source-disclosure-toggle" aria-hidden="true">
                  <span className="source-disclosure-toggle-open">Hide</span>
                  <span className="source-disclosure-toggle-closed">Open</span>
                </span>
              </div>
            </summary>

            <div className="source-panel-body">
              {panel.items.map((item) => (
                <details
                  key={`${panel.title}-${item.label}`}
                  className="source-fact-disclosure"
                >
                  <summary className="source-fact-row">
                    <div>
                      <span>{item.label}</span>
                      <strong>{item.value}</strong>
                    </div>
                    <div className="source-fact-meta">
                      <span>{item.source_type} · {item.confidence_pct}% · {formatSourceDate(item.learned_at)}</span>
                      <span className="source-disclosure-toggle" aria-hidden="true">
                        <span className="source-disclosure-toggle-open">Hide</span>
                        <span className="source-disclosure-toggle-closed">Open</span>
                      </span>
                    </div>
                  </summary>

                  <div className="source-fact-detail">
                    <blockquote>{item.value}</blockquote>
                    <dl>
                      <div>
                        <dt>Type</dt>
                        <dd>{item.source_type}</dd>
                      </div>
                      <div>
                        <dt>Confidence</dt>
                        <dd>{item.confidence_pct}%</dd>
                      </div>
                      <div>
                        <dt>Stored</dt>
                        <dd>{formatSourceDate(item.learned_at)}</dd>
                      </div>
                    </dl>
                    {item.source_url && (
                      <a className="source-fact-link" href={item.source_url} target="_blank" rel="noreferrer">
                        Open source
                      </a>
                    )}
                  </div>
                </details>
              ))}

              {panel.missing.length > 0 && (
                <div className="source-missing-list">
                  <span>Not captured</span>
                  {panel.missing.map((item) => (
                    <p key={item}>{item}</p>
                  ))}
                </div>
              )}
            </div>
          </details>
        ))}
      </div>
    </section>
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
