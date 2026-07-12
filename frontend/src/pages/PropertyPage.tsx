import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { PropertyDetailResponse, SellerSummary } from "../lib/types.ts";
import { getProperty, submitClaim, expressInterest } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";
import { ShareButtons } from "../components/ShareButtons.tsx";
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

type DecisionTone = "keep" | "verify" | "negotiate";

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
  nextAction: string;
} {
  const p = data.property;
  const delta = normalizedDelta(data.market_activity.price_vs_median?.pct_diff);
  const trust = trustPercent(data);
  const risks = riskSignalsFor(p);
  const topRisk = risks[0];
  const topRiskLabel = riskLabel(topRisk.value);

  if (trust < 60) {
    return {
      label: "Verify before shortlisting",
      tone: "verify",
      summary: "The home may fit, but the source chain is not strong enough yet.",
      nextAction: "Verify seller source and documents before scheduling a final visit.",
    };
  }

  if (topRiskLabel === "High") {
    return {
      label: "Verify risk before visit",
      tone: "verify",
      summary: `${topRisk.label} risk is the main blocker. Clear that before treating this as a finalist.`,
      nextAction: `Resolve ${topRisk.label.toLowerCase()} evidence before final visit.`,
    };
  }

  if (delta !== null && delta > 8) {
    return {
      label: "Negotiate before final visit",
      tone: "negotiate",
      summary: "The home is above the local benchmark, so it needs a sharper comp-backed price conversation.",
      nextAction: "Use area median and recent comps as the negotiation anchor.",
    };
  }

  return {
    label: "Good shortlist candidate",
    tone: "keep",
    summary: "Price, trust, and risk are balanced enough to keep this in the decision sheet.",
    nextAction: "Verify tower documents and access-road conditions before final visit.",
  };
}

function shortlistChecks(data: PropertyDetailResponse): string[] {
  const p = data.property;
  const riskChecks = riskSignalsFor(p)
    .filter((risk) => risk.value >= 0.25)
    .slice(0, 2)
    .map((risk) => `Check ${risk.label.toLowerCase()} evidence`);
  const explicit = data.tradeoffs.cautions.slice(0, 2);
  return Array.from(new Set([...explicit, ...riskChecks])).slice(0, 3);
}

export function PropertyPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!id) return;
    queueMicrotask(() => setSaved(isShortlisted(id)));
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
  const medianDelta = normalizedDelta(pvm?.pct_diff);
  const trust = trustPercent(data);
  const trustLabel = data.confidence_score?.label ?? (trust >= 75 ? "High" : trust >= 55 ? "Medium" : "Low");
  const risks = riskSignalsFor(p);
  const topRisk = risks[0];
  const checks = shortlistChecks(data);
  const sourceLabel = data.root_source === "rera" ? "RERA-rooted source" : data.root_source === "seller" ? "Seller source" : "Source pending";
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
            {saved ? "\u2665 In decision sheet" : "\u2661 Save to sheet"}
          </button>
        </div>
      </section>

      <div className="property-decision-layout">
        <main className="property-decision-main">
          <section className="property-decision-card property-decision-card--lead">
            <div className="property-section-heading">
              <span>Decision brief</span>
              <h2>What this means for a buyer</h2>
            </div>

            <div className="property-decision-metrics">
              <DecisionMetric
                label="Value"
                value={formatMedianDelta(medianDelta)}
                detail={area ? `Area median ₹${area.median_price_per_sqft.toLocaleString("en-IN")}/sqft` : "Area benchmark unavailable"}
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
                label="Next action"
                value={decision.nextAction}
                detail="Use this before scheduling or negotiating."
                tone={decision.tone === "keep" ? "good" : "watch"}
              />
            </div>

            <div className="property-brief-columns">
              <div>
                <h3>Why keep it</h3>
                <ul className="property-check-list property-check-list--positive">
                  {(tradeoffs.strengths.length > 0 ? tradeoffs.strengths : ["Fits the active shortlist profile."]).slice(0, 3).map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
              <div>
                <h3>Verify before moving</h3>
                <ul className="property-check-list property-check-list--watch">
                  {checks.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
            </div>
          </section>

          <section className="property-evidence-section">
            <div className="property-section-heading">
              <span>Evidence</span>
              <h2>What supports the decision</h2>
            </div>

            <div className="property-evidence-grid">
              <div className="property-evidence-card">
                <h3>Price and market</h3>
                <EvidenceRow label="Ask" value={formatPrice(p.price)} detail={pricePerSqftLabel ?? "Rate per sqft not available"} />
                <EvidenceRow label="Benchmark" value={formatMedianDelta(medianDelta)} detail={area ? `${area.name} median: ₹${area.median_price_per_sqft.toLocaleString("en-IN")}/sqft` : "No area benchmark"} />
                <EvidenceRow label="Demand" value={market_activity.interest_label} detail={`${market_activity.days_on_market} days on market`} />
              </div>

              <div className="property-evidence-card">
                <h3>Source and documents</h3>
                <EvidenceRow label="Trust" value={`${trust}/100`} detail={sourceLabel} />
                <EvidenceRow label="RERA" value={data.rera?.registered ? "Registered" : "Verification pending"} detail={data.rera?.registration_number ?? "Confirm before token or legal review"} />
                <EvidenceRow
                  label="Documents"
                  value={`${Math.round(p.document_completeness_score * 100)}% complete`}
                  detail={data.rera ? "RERA and source documents are available for legal review." : "Ask for sale deed, khata, OC/CC, and dues before token."}
                />
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
          </section>

          {(society || area) && (
            <section className="property-context-panel">
              <div className="property-section-heading">
                <span>Context</span>
                <h2>What changes the lived experience</h2>
              </div>

              <div className="property-context-grid">
                {society && (
                  <div>
                    <h3>{society.name}</h3>
                    <p>{society.review_summary || society.summary}</p>
                    <div className="property-context-pills">
                      <span>Builder: {society.builder_name}</span>
                      <span>{society.year_built}</span>
                      <span>{society.maintenance_sentiment}</span>
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
                    <p>{area.trend_summary}</p>
                    <div className="property-context-pills">
                      <span>₹{area.median_price_per_sqft.toLocaleString("en-IN")} /sqft median</span>
                      <span>{area.trend_direction}</span>
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
                <span>Alternatives</span>
                <h2>Keep pressure on this choice</h2>
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

          {!data.seller && <ClaimSection propertyId={p.id} />}
        </main>

        <aside className="property-action-rail">
          <div className="property-action-card">
            <span>Next action</span>
            <strong>{decision.nextAction}</strong>
            <button
              onClick={handleSave}
              data-testid="sidebar-save-button"
              className={`btn ${saved ? "btn-primary" : "btn-outline"}`}
            >
              {saved ? "\u2665 In decision sheet" : "\u2661 Save to sheet"}
            </button>
            <ShareButtons propertyId={p.id} title={p.title} />
          </div>

          <InterestButton propertyId={p.id} initialCount={data.interest_count ?? 0} />
          {data.seller && <SellerInfoCard seller={data.seller} />}

          <div className="property-mini-card property-rail-intel">
            <div>
              <h3>Risk watchlist</h3>
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
