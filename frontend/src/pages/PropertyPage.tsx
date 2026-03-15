import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { PropertyDetailResponse, ThemeLabel, SellerSummary } from "../lib/types.ts";
import { getProperty, submitClaim, expressInterest } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";
import { ReraTile, ReraPendingTile } from "../components/ReraTile.tsx";
import { TransparencyScoreTile } from "../components/TransparencyScoreTile.tsx";
import { ShareButtons } from "../components/ShareButtons.tsx";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";
import { ConfidenceMeter } from "../components/ConfidenceMeter.tsx";
import { AreaIntelligenceTile } from "../components/AreaIntelligenceTile.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

function ScoreBar({ label, value }: { label: string; value: number }) {
  const pct = Math.round(value * 100);
  const color = pct >= 70 ? "var(--color-positive)" : pct >= 40 ? "var(--color-warning)" : "var(--color-negative)";
  return (
    <div className="score-bar">
      <div className="score-bar-label">
        <span>{label}</span>
        <span style={{ fontWeight: 600, color }}>{pct}%</span>
      </div>
      <div className="score-bar-track">
        <div className="score-bar-fill" style={{ width: `${pct}%`, backgroundColor: color }} />
      </div>
    </div>
  );
}

function ThemeBadge({ level }: { level: ThemeLabel }) {
  const config: Record<ThemeLabel, { bg: string; color: string; border: string }> = {
    strong: { bg: "var(--color-positive-bg)", color: "var(--color-positive)", border: "var(--color-positive-border)" },
    good: { bg: "#f0f5f0", color: "#3a8a3a", border: "#c0d8c0" },
    mixed: { bg: "#fdf8f0", color: "var(--color-warning)", border: "#e8dcc0" },
    weak: { bg: "var(--color-negative-bg)", color: "var(--color-negative)", border: "var(--color-negative-border)" },
  };
  const c = config[level];
  return (
    <span style={{
      display: "inline-block",
      fontSize: "0.7rem",
      fontWeight: 600,
      padding: "0.15rem 0.5rem",
      borderRadius: "var(--radius-xl)",
      backgroundColor: c.bg,
      color: c.color,
      border: `1px solid ${c.border}`,
      textTransform: "capitalize",
    }}>
      {level}
    </span>
  );
}

function buildPropertyJsonLd(p: PropertyDetailResponse["property"]) {
  const jsonLd: Record<string, unknown> = {
    "@context": "https://schema.org",
    "@type": "RealEstateListing",
    name: p.title,
    description: p.description_summary || `${p.bhk} BHK, ${p.carpet_area_sqft} sqft in ${p.area}, ${p.city}`,
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
    floorSize: {
      "@type": "QuantitativeValue",
      value: p.carpet_area_sqft,
      unitCode: "FTK",
    },
    numberOfRooms: p.bhk,
  };
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
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!id) return;
    setSaved(isShortlisted(id));
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

  const { property: p, society, area, tradeoffs, market_activity } = data;
  const pvm = market_activity.price_vs_median;

  const handleSave = () => {
    if (!id) return;
    toggleShortlist(id);
    setSaved(!saved);
  };

  const pageTitle = `${p.title} — ${p.bhk} BHK in ${p.area} | OpenEstates`;
  const pageDescription = `${p.bhk} BHK, ${p.carpet_area_sqft} sqft in ${society?.name ? society.name + ", " : ""}${p.area}. ${formatPrice(p.price)} (${p.price_per_sqft.toLocaleString("en-IN")}/sqft). Transparency scores, risk signals, and tradeoffs.`;

  return (
    <div className="page-container-wide">
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
        className="back-link"
        style={{ background: "none", border: "none", cursor: "pointer", padding: 0 }}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
        Back to results
      </button>

      {/* Hero image */}
      <ImageWithFallback
        src={p.hero_image}
        alt={p.title}
        className="property-hero-image"
        loading="eager"
      />

      {/* === A. Property Summary === */}
      <div style={{ marginTop: "1.5rem", marginBottom: "2rem" }}>
        <div>
          <h1 style={{
            fontSize: "clamp(1.5rem, 1.2rem + 1vw, 2rem)",
            fontWeight: 700,
            letterSpacing: "-0.025em",
            margin: "0 0 0.35rem",
          }}>
            {p.title}
          </h1>
          <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", flexWrap: "wrap", margin: "0 0 1rem" }}>
            <p style={{ color: "var(--color-text-secondary)", margin: 0, fontSize: "0.95rem" }}>
              {society?.name ? `${society.name} \u00B7 ` : ""}{p.area}, {p.city}
            </p>
            <ConfidenceMeter confidence={data.confidence_score} />
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "baseline", gap: "0.75rem", flexWrap: "wrap" }}>
          <span style={{ fontSize: "1.75rem", fontWeight: 700, letterSpacing: "-0.02em" }}>
            {formatPrice(p.price)}
          </span>
          <span style={{ color: "var(--color-text-muted)", fontSize: "0.9rem" }}>
            {p.price_per_sqft.toLocaleString("en-IN")} /sqft
          </span>
        </div>

        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap", marginTop: "0.75rem" }}>
          <span className="tag tag-neutral">{p.bhk} BHK</span>
          <span className="tag tag-neutral">{p.carpet_area_sqft} sqft carpet</span>
          <span className="tag tag-neutral">{p.facing} facing</span>
          <span className="tag tag-neutral">Floor {p.floor}/{p.total_floors}</span>
          <ProjectStatusTag
            status={data.project_status}
            displayText={data.project_status_display}
            possessionStatus={p.possession_status}
          />
        </div>
      </div>

      {/* === Two-column layout: Main content + Sticky sidebar === */}
      <div className="property-layout">
        {/* === Main column === */}
        <div className="property-main">

      {/* === B. Why This Property For You === */}
      <div className="section-card" data-testid="why-this-property-section">
        <div className="section-card-header">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
            <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
          </svg>
          <h2>Why this property for you</h2>
        </div>
        <p style={{ margin: "0 0 1.25rem", fontSize: "1.05rem", lineHeight: 1.6, color: "var(--color-text)", fontWeight: 500 }}>
          {tradeoffs.headline}
        </p>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))", gap: "0.75rem" }}>
          {tradeoffs.components.map((c) => (
            <div key={c.label} style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              padding: "0.6rem 0.85rem",
              borderRadius: "var(--radius-sm)",
              backgroundColor: "var(--color-bg-elevated)",
              border: "1px solid var(--color-border)",
            }}>
              <span style={{ fontSize: "0.8rem", color: "var(--color-text-secondary)" }}>{c.label}</span>
              <ThemeBadge level={c.level} />
            </div>
          ))}
        </div>

        {/* Strengths & Cautions inline */}
        {(tradeoffs.strengths.length > 0 || tradeoffs.cautions.length > 0) && (
          <div className="detail-grid" style={{ gap: "1rem", marginTop: "1rem" }}>
            {tradeoffs.strengths.length > 0 && (
              <div style={{
                padding: "1rem",
                borderRadius: "var(--radius-sm)",
                backgroundColor: "var(--color-positive-bg)",
                border: "1px solid var(--color-positive-border)",
              }}>
                <h3 style={{
                  margin: "0 0 0.5rem",
                  fontSize: "0.7rem",
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: "var(--color-positive)",
                }}>
                  Strengths
                </h3>
                <ul className="insight-list insight-list-positive">
                  {tradeoffs.strengths.map((s, i) => <li key={i}>{s}</li>)}
                </ul>
              </div>
            )}
            {tradeoffs.cautions.length > 0 && (
              <div style={{
                padding: "1rem",
                borderRadius: "var(--radius-sm)",
                backgroundColor: "var(--color-negative-bg)",
                border: "1px solid var(--color-negative-border)",
              }}>
                <h3 style={{
                  margin: "0 0 0.5rem",
                  fontSize: "0.7rem",
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: "var(--color-negative)",
                }}>
                  Cautions
                </h3>
                <ul className="insight-list insight-list-negative">
                  {tradeoffs.cautions.map((c, i) => <li key={i}>{c}</li>)}
                </ul>
              </div>
            )}
          </div>
        )}
      </div>

      {/* === RERA Verification === */}
      {data.rera ? <ReraTile rera={data.rera} /> : <ReraPendingTile />}

      {/* === Key facts card === */}
      <div className="section-card">
        <div className="section-card-header">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
            <rect x="3" y="3" width="7" height="7" />
            <rect x="14" y="3" width="7" height="7" />
            <rect x="3" y="14" width="7" height="7" />
            <rect x="14" y="14" width="7" height="7" />
          </svg>
          <h2>Property details</h2>
        </div>
        <div className="fact-grid">
          <Fact label="Configuration" value={`${p.bhk} BHK`} />
          <Fact label="Carpet area" value={`${p.carpet_area_sqft} sqft`} />
          <Fact label="Super built-up" value={`${p.super_builtup_sqft} sqft`} />
          <Fact label="Floor" value={`${p.floor} of ${p.total_floors}`} />
          <Fact label="Facing" value={p.facing} />
          <Fact label="Possession" value={p.possession_status.replace(/_/g, " ")} />
          <Fact label="Metro distance" value={`${p.metro_distance_mins} min`} />
          <Fact label="Maintenance" value={`\u20B9${p.maintenance_cost_monthly.toLocaleString("en-IN")}/mo`} />
        </div>
      </div>

      {/* === Risk signals === */}
      <div className="section-card">
        <div className="section-card-header">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
            <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
            <line x1="12" y1="9" x2="12" y2="13" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
          <h2>Risk signals</h2>
        </div>
        <ScoreBar label="Litigation risk" value={p.litigation_risk} />
        <ScoreBar label="Waterlogging risk" value={p.waterlogging_risk_score} />
        <ScoreBar label="Traffic congestion" value={p.traffic_score} />
        <ScoreBar label="Noise level" value={p.noise_score} />
      </div>

      {/* === E. Society / Livability === */}
      {society && (
        <div className="section-card" data-testid="society-livability-section">
          <div className="section-card-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
              <polyline points="9 22 9 12 15 12 15 22" />
            </svg>
            <h2>Society / Livability &middot; {society.name}</h2>
          </div>

          <p style={{ margin: "0 0 1rem", lineHeight: 1.6, color: "var(--color-text)", fontSize: "0.95rem" }}>
            {society.summary}
          </p>

          <div style={{ display: "flex", gap: "1.5rem", flexWrap: "wrap", marginBottom: "1.25rem" }}>
            <SocietyMeta label="Builder" value={society.builder_name} />
            {data.builder_trust?.delivery_display && (
              <div style={{ display: "flex", alignItems: "center" }}>
                <BuilderTrustBadge
                  deliveryDisplay={data.builder_trust.delivery_display}
                  deliveryRate={data.builder_trust.delivery_rate}
                />
              </div>
            )}
            <SocietyMeta label="Year built" value={String(society.year_built)} />
            <SocietyMeta label="Maintenance" value={society.maintenance_sentiment} />
            <SocietyMeta label="Livability" value={society.livability_sentiment} />
          </div>

          {(society.common_positives.length > 0 || society.common_complaints.length > 0) && (
            <div className="detail-grid" style={{ gap: "1rem" }}>
              {society.common_positives.length > 0 && (
                <div style={{
                  padding: "1.25rem",
                  borderRadius: "var(--radius-sm)",
                  backgroundColor: "var(--color-positive-bg)",
                  border: "1px solid var(--color-positive-border)",
                }}>
                  <h3 style={{
                    margin: "0 0 0.5rem",
                    fontSize: "0.75rem",
                    fontWeight: 600,
                    textTransform: "uppercase",
                    letterSpacing: "0.08em",
                    color: "var(--color-positive)",
                  }}>
                    What residents like
                  </h3>
                  <ul className="insight-list insight-list-positive">
                    {society.common_positives.map((item, i) => <li key={i}>{item}</li>)}
                  </ul>
                </div>
              )}
              {society.common_complaints.length > 0 && (
                <div style={{
                  padding: "1.25rem",
                  borderRadius: "var(--radius-sm)",
                  backgroundColor: "var(--color-negative-bg)",
                  border: "1px solid var(--color-negative-border)",
                }}>
                  <h3 style={{
                    margin: "0 0 0.5rem",
                    fontSize: "0.75rem",
                    fontWeight: 600,
                    textTransform: "uppercase",
                    letterSpacing: "0.08em",
                    color: "var(--color-negative)",
                  }}>
                    Common concerns
                  </h3>
                  <ul className="insight-list insight-list-negative">
                    {society.common_complaints.map((item, i) => <li key={i}>{item}</li>)}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* === F. Area Signals (with greenery/openness) === */}
      {area && (
        <div className="section-card" data-testid="area-signals-section" style={{ marginBottom: "3rem" }}>
          <div className="section-card-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
              <circle cx="12" cy="10" r="3" />
            </svg>
            <h2>Area signals &middot; {area.name}</h2>
          </div>

          <div style={{ display: "flex", alignItems: "baseline", gap: "0.75rem", marginBottom: "1.25rem" }}>
            <span style={{ fontSize: "1.2rem", fontWeight: 700 }}>
              {area.median_price_per_sqft.toLocaleString("en-IN")}
            </span>
            <span style={{ color: "var(--color-text-muted)", fontSize: "0.85rem" }}>/sqft median</span>
            <span className={`tag ${area.trend_direction === "up" ? "tag-positive" : "tag-neutral"}`}>
              {area.trend_direction === "up" ? "\u2197" : "\u2192"} {area.trend_direction}
            </span>
          </div>

          <div style={{ display: "grid", gap: "0.75rem" }}>
            {area.metro_access_summary && <AreaInsight icon="train" text={area.metro_access_summary} />}
            {area.traffic_summary && <AreaInsight icon="car" text={area.traffic_summary} />}
            {area.waterlogging_summary && <AreaInsight icon="water" text={area.waterlogging_summary} />}

            {/* Greenery / Openness — from backend themes */}
            <AreaInsight
              icon="tree"
              text={data.themes.greenery.summary}
            />

            {area.livability_summary && <AreaInsight icon="home" text={area.livability_summary} />}
          </div>

          {area.community_notes && (
            <div style={{
              marginTop: "1rem",
              padding: "1rem",
              borderRadius: "var(--radius-sm)",
              backgroundColor: "var(--color-bg-elevated)",
              border: "1px solid var(--color-border)",
            }}>
              <p style={{
                margin: 0,
                fontSize: "0.85rem",
                color: "var(--color-text-secondary)",
                fontStyle: "italic",
                lineHeight: 1.6,
              }}>
                &ldquo;{area.community_notes}&rdquo;
              </p>
              <p style={{
                margin: "0.5rem 0 0",
                fontSize: "0.7rem",
                color: "var(--color-text-muted)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
              }}>
                Community notes
              </p>
            </div>
          )}
        </div>
      )}

      {/* === Area Intelligence (Reddit sentiments) === */}
      {data.area_intelligence && area && (
        <AreaIntelligenceTile area={area.name} intelligence={data.area_intelligence} />
      )}

      {/* === Claim Section — only shown when no seller is linked === */}
      {!data.seller && <ClaimSection propertyId={p.id} />}

        </div>{/* end property-main */}

        {/* === Sticky sidebar === */}
        <div className="property-sidebar">
          {/* Transparency Score — top of sidebar */}
          <TransparencyScoreTile data={data.transparency_score} />

          {/* Save + Share */}
          <div className="section-card" style={{ marginBottom: "1rem" }}>
            <button
              onClick={handleSave}
              data-testid="sidebar-save-button"
              className={`btn ${saved ? "btn-primary" : "btn-outline"}`}
              style={{ width: "100%", justifyContent: "center" }}
            >
              {saved ? "\u2665 Saved to shortlist" : "\u2661 Save to shortlist"}
            </button>
            <ShareButtons propertyId={p.id} title={p.title} />
          </div>

          {/* "I'm Interested" button */}
          <InterestButton propertyId={p.id} initialCount={data.interest_count ?? 0} />

          {/* Seller info card (if linked) */}
          {data.seller && <SellerInfoCard seller={data.seller} />}

          {/* Price vs Area Median — spectrum gauge */}
          {pvm && area && (() => {
            const verdictColor = pvm.verdict_class === "positive" ? "var(--color-positive)"
              : pvm.verdict_class === "warning" ? "var(--color-warning)"
              : "var(--color-text-secondary)";

            const rangeLow = data.area_price_range_low ?? area.median_price_per_sqft * 0.8;
            const rangeHigh = data.area_price_range_high ?? area.median_price_per_sqft * 1.2;
            const rangeSpan = rangeHigh - rangeLow || 1;
            const propertyPos = Math.min(Math.max(((p.price_per_sqft - rangeLow) / rangeSpan) * 100, 2), 98);
            const medianPos = Math.min(Math.max(((area.median_price_per_sqft - rangeLow) / rangeSpan) * 100, 2), 98);

            return (
            <div className="section-card" data-testid="price-vs-median-section" style={{ marginBottom: "1rem" }}>
              <div style={{
                fontSize: "0.62rem",
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "var(--color-text-muted)",
                marginBottom: "0.5rem",
              }}>
                Price vs {area.name} median
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.5rem" }}>
                <span style={{ fontSize: "1.1rem", fontWeight: 700 }}>
                  {"\u20B9"}{p.price_per_sqft.toLocaleString("en-IN")}
                </span>
                <span style={{ fontSize: "0.85rem", color: "var(--color-text-muted)" }}>
                  vs {"\u20B9"}{area.median_price_per_sqft.toLocaleString("en-IN")}
                </span>
              </div>

              {/* Spectrum gauge */}
              <div className="price-spectrum" style={{ position: "relative", marginBottom: "0.75rem" }}>
                <div className="price-spectrum-bar" />
                {/* Median marker */}
                <div style={{
                  position: "absolute",
                  left: `${medianPos}%`,
                  top: "-2px",
                  transform: "translateX(-50%)",
                  width: "2px",
                  height: "14px",
                  backgroundColor: "var(--color-text-muted)",
                  borderRadius: "1px",
                }} />
                {/* Property marker */}
                <div style={{
                  position: "absolute",
                  left: `${propertyPos}%`,
                  top: "-4px",
                  transform: "translateX(-50%)",
                  width: "10px",
                  height: "18px",
                  borderRadius: "5px",
                  backgroundColor: verdictColor,
                  border: "2px solid var(--color-bg-card)",
                  boxShadow: "0 1px 3px rgba(0,0,0,0.15)",
                  transition: "left 0.8s var(--ease-out)",
                }} />
              </div>

              {/* Range labels */}
              {data.area_price_range_low && data.area_price_range_high && (
                <div style={{
                  display: "flex",
                  justifyContent: "space-between",
                  fontSize: "0.65rem",
                  color: "var(--color-text-muted)",
                  marginBottom: "0.5rem",
                }}>
                  <span>{"\u20B9"}{data.area_price_range_low.toLocaleString("en-IN")}</span>
                  <span>{"\u20B9"}{data.area_price_range_high.toLocaleString("en-IN")}</span>
                </div>
              )}

              <div style={{ fontSize: "0.8rem", fontWeight: 600, color: verdictColor }}>
                {pvm.verdict} ({pvm.pct_diff > 0 ? "+" : ""}{pvm.pct_diff}%)
              </div>
            </div>
            );
          })()}

          {/* Market Activity */}
          <div className="section-card" data-testid="market-activity-section">
            <div className="section-card-header" style={{ marginBottom: "0.5rem" }}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
              </svg>
              <h2 style={{ fontSize: "0.85rem" }}>Market activity</h2>
            </div>
            <div style={{ display: "grid", gap: "0.5rem" }}>
              <MarketRow
                icon={<InterestDot level={market_activity.interest_level as "high" | "moderate" | "low"} />}
                label={market_activity.interest_label}
              />
              {market_activity.saves_last_7d != null && (
                <MarketRow
                  icon={<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" /></svg>}
                  label={`${market_activity.saves_last_7d} saves this week`}
                />
              )}
              {market_activity.offers_last_7d != null && market_activity.offers_last_7d > 0 && (
                <MarketRow
                  icon={<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><rect x="2" y="7" width="20" height="14" rx="2" ry="2" /><path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16" /></svg>}
                  label={`${market_activity.offers_last_7d} offer${market_activity.offers_last_7d > 1 ? "s" : ""} this week`}
                />
              )}
              <MarketRow
                icon={<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>}
                label={`Listed ${market_activity.days_on_market}d ago \u00B7 ${market_activity.days_on_market_label}`}
              />
              {market_activity.area_trend_summary && (
                <MarketRow
                  icon={<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><polyline points="23 6 13.5 15.5 8.5 10.5 1 18" /><polyline points="17 6 23 6 23 12" /></svg>}
                  label={market_activity.area_trend_summary}
                />
              )}
            </div>
          </div>
        </div>{/* end property-sidebar */}
      </div>{/* end property-layout */}

      {/* Similar properties — embedding-based */}
      {data.similar_properties.length > 0 && (
        <div className="card" style={{ marginTop: "1.5rem" }}>
          <h2 className="section-title">You might also like</h2>
          <div style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
            gap: "1rem",
            marginTop: "1rem",
          }}>
            {data.similar_properties.map((sp) => (
              <Link
                key={sp.id}
                to={`/property/${sp.id}`}
                style={{ textDecoration: "none", color: "inherit" }}
              >
                <div style={{
                  borderRadius: "var(--radius-md)",
                  border: "1px solid var(--color-border)",
                  overflow: "hidden",
                  backgroundColor: "var(--color-bg-elevated)",
                  transition: "border-color 0.15s",
                }}>
                  <ImageWithFallback
                    src={sp.hero_image || ""}
                    alt={sp.title}
                    style={{ width: "100%", height: "140px", objectFit: "cover" }}
                  />
                  <div style={{ padding: "0.75rem" }}>
                    <p style={{
                      margin: 0,
                      fontWeight: 600,
                      fontSize: "0.85rem",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}>
                      {sp.title}
                    </p>
                    <p style={{
                      margin: "0.25rem 0 0",
                      fontSize: "0.8rem",
                      color: "var(--color-text-muted)",
                    }}>
                      {sp.society_name} · {sp.area}
                    </p>
                    <p style={{
                      margin: "0.25rem 0 0",
                      fontWeight: 600,
                      fontSize: "0.85rem",
                      color: "var(--color-primary)",
                    }}>
                      {formatPrice(sp.price)}
                    </p>
                  </div>
                </div>
              </Link>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function ClaimSection({ propertyId }: { propertyId: string }) {
  const [expanded, setExpanded] = useState(false);
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "success" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setStatus("submitting");
    setErrorMsg("");
    try {
      await submitClaim({
        property_id: propertyId,
        name: name.trim(),
        phone: phone.trim() || undefined,
        email: email.trim() || undefined,
      });
      setStatus("success");
    } catch (err: unknown) {
      setStatus("error");
      setErrorMsg(err instanceof Error ? err.message : "Something went wrong");
    }
  };

  if (status === "success") {
    return (
      <div className="claim-section" style={{ marginTop: "1.5rem" }}>
        <div style={{
          display: "flex",
          alignItems: "center",
          gap: "0.5rem",
          color: "var(--color-positive)",
          fontWeight: 600,
          fontSize: "0.95rem",
        }}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
            <polyline points="22 4 12 14.01 9 11.01" />
          </svg>
          Claim submitted. We'll be in touch.
        </div>
      </div>
    );
  }

  return (
    <div className="claim-section" style={{ marginTop: "1.5rem" }}>
      {!expanded ? (
        <div style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          flexWrap: "wrap",
          gap: "0.75rem",
        }}>
          <div>
            <div style={{ fontWeight: 600, fontSize: "0.95rem", color: "var(--color-text)" }}>
              Is this your property?
            </div>
            <div style={{ fontSize: "0.82rem", color: "var(--color-text-muted)", marginTop: "0.15rem" }}>
              Verify ownership and manage your listing
            </div>
          </div>
          <button
            onClick={() => setExpanded(true)}
            className="btn btn-outline"
            style={{ flexShrink: 0 }}
          >
            Claim it
          </button>
        </div>
      ) : (
        <form onSubmit={handleSubmit}>
          <div style={{ fontWeight: 600, fontSize: "0.95rem", marginBottom: "0.75rem", color: "var(--color-text)" }}>
            Claim this property
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
            <input
              type="text"
              className="claim-input"
              placeholder="Your name *"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
            />
            <input
              type="tel"
              className="claim-input"
              placeholder="Phone number"
              value={phone}
              onChange={(e) => setPhone(e.target.value)}
            />
            <input
              type="email"
              className="claim-input"
              placeholder="Email address"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </div>
          {status === "error" && (
            <div style={{
              marginTop: "0.5rem",
              fontSize: "0.82rem",
              color: "var(--color-negative)",
            }}>
              {errorMsg || "Failed to submit claim. Please try again."}
            </div>
          )}
          <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.85rem" }}>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={status === "submitting" || !name.trim() || (!phone.trim() && !email.trim())}
            >
              {status === "submitting" ? "Submitting..." : "Submit claim"}
            </button>
            <button
              type="button"
              className="btn btn-outline"
              onClick={() => { setExpanded(false); setStatus("idle"); setErrorMsg(""); }}
            >
              Cancel
            </button>
          </div>
        </form>
      )}
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="fact-item-label">{label}</div>
      <div className="fact-item-value">{value}</div>
    </div>
  );
}

function SocietyMeta({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div style={{ fontSize: "0.7rem", color: "var(--color-text-muted)", textTransform: "uppercase", letterSpacing: "0.06em" }}>
        {label}
      </div>
      <div style={{ fontSize: "0.9rem", fontWeight: 500 }}>{value}</div>
    </div>
  );
}

function MarketRow({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <div style={{
      display: "flex",
      gap: "0.75rem",
      alignItems: "center",
      padding: "0.6rem 0.85rem",
      borderRadius: "var(--radius-sm)",
      backgroundColor: "var(--color-bg-elevated)",
      border: "1px solid var(--color-border)",
    }}>
      <span style={{ color: "var(--color-text-muted)", flexShrink: 0 }}>{icon}</span>
      <span style={{ fontSize: "0.8rem", color: "var(--color-text)", lineHeight: 1.4 }}>{label}</span>
    </div>
  );
}

function InterestDot({ level }: { level: "high" | "moderate" | "low" }) {
  const color = level === "high" ? "var(--color-positive)" : level === "moderate" ? "var(--color-warning)" : "var(--color-text-muted)";
  return (
    <span style={{
      display: "inline-block",
      width: "8px",
      height: "8px",
      borderRadius: "50%",
      backgroundColor: color,
    }} />
  );
}

function AreaInsight({ icon, text }: { icon: string; text: string }) {
  const icons: Record<string, React.ReactNode> = {
    trending: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <polyline points="23 6 13.5 15.5 8.5 10.5 1 18" />
        <polyline points="17 6 23 6 23 12" />
      </svg>
    ),
    train: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <rect x="4" y="3" width="16" height="16" rx="2" />
        <line x1="4" y1="11" x2="20" y2="11" />
        <line x1="12" y1="3" x2="12" y2="11" />
        <line x1="8" y1="23" x2="8" y2="19" />
        <line x1="16" y1="23" x2="16" y2="19" />
      </svg>
    ),
    car: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <path d="M14 16H9m10 0h3v-3.15a1 1 0 0 0-.84-.99L16 11l-2.7-3.6a1 1 0 0 0-.8-.4H5.24a2 2 0 0 0-1.8 1.1l-.8 1.63A6 6 0 0 0 2 12.42V16h2" />
        <circle cx="6.5" cy="16.5" r="2.5" />
        <circle cx="16.5" cy="16.5" r="2.5" />
      </svg>
    ),
    home: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      </svg>
    ),
    tree: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 22v-7" />
        <path d="M7 15l5-11 5 11H7z" />
      </svg>
    ),
    water: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <path d="M12 2.69l5.66 5.66a8 8 0 1 1-11.31 0z" />
      </svg>
    ),
  };

  return (
    <div style={{
      display: "flex",
      gap: "0.75rem",
      alignItems: "flex-start",
      padding: "0.75rem 1rem",
      borderRadius: "var(--radius-sm)",
      backgroundColor: "var(--color-bg-elevated)",
      border: "1px solid var(--color-border)",
    }}>
      <span style={{ color: "var(--color-text-muted)", marginTop: "0.15rem", flexShrink: 0 }}>
        {icons[icon] || icons.home}
      </span>
      <p style={{ margin: 0, fontSize: "0.9rem", color: "var(--color-text)", lineHeight: 1.5 }}>
        {text}
      </p>
    </div>
  );
}

const INTEREST_KEY_PREFIX = "oe_interest_";

function InterestButton({ propertyId, initialCount }: { propertyId: string; initialCount: number }) {
  const storageKey = `${INTEREST_KEY_PREFIX}${propertyId}`;
  const alreadySent = localStorage.getItem(storageKey) === "1";

  const [status, setStatus] = useState<"idle" | "submitting" | "success" | "already_expressed" | "error">(
    alreadySent ? "already_expressed" : "idle"
  );
  const [count, setCount] = useState(initialCount);

  const handleClick = async () => {
    if (status !== "idle" && status !== "error") return;
    setStatus("submitting");
    try {
      await expressInterest({ property_id: propertyId });
      localStorage.setItem(storageKey, "1");
      setStatus("success");
      setCount((c) => c + 1);
    } catch {
      setStatus("error");
      setTimeout(() => setStatus("idle"), 3000);
    }
  };

  const isDisabled = status === "submitting" || status === "success" || status === "already_expressed";
  const buttonLabel =
    status === "submitting" ? "Sending..." :
    status === "success" || status === "already_expressed" ? "Interest sent" :
    status === "error" ? "Try Again" :
    "I'm Interested";

  return (
    <div style={{ marginBottom: "1rem" }}>
      <button
        onClick={handleClick}
        disabled={isDisabled}
        className={`btn ${isDisabled ? "btn-primary" : "btn-outline"}`}
        style={{
          width: "100%",
          justifyContent: "center",
          opacity: status === "submitting" ? 0.7 : 1,
        }}
      >
        {status === "success" || status === "already_expressed" ? (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" style={{ marginRight: "0.4rem" }}>
            <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
            <polyline points="22 4 12 14.01 9 11.01" />
          </svg>
        ) : (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" style={{ marginRight: "0.4rem" }}>
            <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
          </svg>
        )}
        {buttonLabel}
      </button>
      {status === "error" && (
        <div style={{
          marginTop: "0.5rem",
          fontSize: "0.78rem",
          color: "var(--color-negative, #ef4444)",
          textAlign: "center",
          lineHeight: 1.4,
        }}>
          Something went wrong. You can try again.
        </div>
      )}
      {count > 0 && (
        <div style={{
          marginTop: "0.5rem",
          fontSize: "0.78rem",
          color: "var(--color-text-muted)",
          textAlign: "center",
        }}>
          {count} buyer{count !== 1 ? "s" : ""} interested
        </div>
      )}
    </div>
  );
}

function SellerInfoCard({ seller }: { seller: SellerSummary }) {
  const completenessColor =
    seller.completeness_pct >= 70 ? "var(--color-positive)" :
    seller.completeness_pct >= 42 ? "var(--color-warning)" :
    "var(--color-negative)";

  return (
    <div className="section-card" style={{ marginBottom: "1rem" }}>
      <div className="section-card-header" style={{ marginBottom: "0.5rem" }}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
          <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
        <h2 style={{ fontSize: "0.85rem" }}>Listed by</h2>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", marginBottom: "0.75rem" }}>
        <span style={{ fontWeight: 600, fontSize: "0.95rem" }}>{seller.name}</span>
        {seller.verified && (
          <span style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "0.2rem",
            fontSize: "0.68rem",
            fontWeight: 600,
            padding: "0.1rem 0.45rem",
            borderRadius: "var(--radius-xl)",
            backgroundColor: "var(--color-positive-bg)",
            color: "var(--color-positive)",
            border: "1px solid var(--color-positive-border)",
          }}>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round">
              <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
              <polyline points="22 4 12 14.01 9 11.01" />
            </svg>
            Verified
          </span>
        )}
      </div>

      {/* Completeness bar */}
      <div style={{ marginBottom: "0.75rem" }}>
        <div style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: "0.72rem",
          color: "var(--color-text-muted)",
          marginBottom: "0.25rem",
        }}>
          <span>Profile completeness</span>
          <span style={{ fontWeight: 600, color: completenessColor }}>{seller.completeness_pct}%</span>
        </div>
        <div style={{
          height: "4px",
          borderRadius: "2px",
          backgroundColor: "var(--color-border)",
          overflow: "hidden",
        }}>
          <div style={{
            height: "100%",
            width: `${seller.completeness_pct}%`,
            backgroundColor: completenessColor,
            borderRadius: "2px",
            transition: "width 0.5s var(--ease-out)",
          }} />
        </div>
      </div>

      {/* Property prompt */}
      {seller.property_prompt && (
        <div style={{
          padding: "0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "var(--color-bg-elevated)",
          border: "1px solid var(--color-border)",
          marginBottom: "0.75rem",
        }}>
          <p style={{
            margin: 0,
            fontSize: "0.82rem",
            color: "var(--color-text-secondary)",
            fontStyle: "italic",
            lineHeight: 1.5,
          }}>
            &ldquo;{seller.property_prompt}&rdquo;
          </p>
          <p style={{
            margin: "0.4rem 0 0",
            fontSize: "0.65rem",
            color: "var(--color-text-muted)",
            textTransform: "uppercase",
            letterSpacing: "0.06em",
          }}>
            Seller's note
          </p>
        </div>
      )}

      {/* Documents provided */}
      {seller.documents_provided.length > 0 && (
        <div style={{ marginBottom: "0.75rem" }}>
          <div style={{
            fontSize: "0.68rem",
            fontWeight: 600,
            textTransform: "uppercase",
            letterSpacing: "0.06em",
            color: "var(--color-text-muted)",
            marginBottom: "0.35rem",
          }}>
            Documents provided
          </div>
          <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
            {seller.documents_provided.map((doc) => (
              <span key={doc} className="tag tag-neutral" style={{ fontSize: "0.72rem" }}>
                {doc}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* View seller profile link */}
      <Link
        to={`/seller/${seller.id}`}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "0.35rem",
          fontSize: "0.82rem",
          fontWeight: 600,
          color: "var(--color-accent, #c96b4f)",
          textDecoration: "none",
          marginTop: "0.25rem",
        }}
      >
        View seller profile
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <path d="M5 12h14M12 5l7 7-7 7" />
        </svg>
      </Link>
    </div>
  );
}
