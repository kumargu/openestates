/**
 * Slide-over side panel for quick property preview.
 * Opens from the right, keeps results grid visible.
 */
import { Fragment, useCallback, useEffect, useState, useRef } from "react";
import { Link } from "react-router-dom";
import type { PropertyDetailResponse, PropertyCard as PropertyCardType } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { isOnSheet, toggleSheetItem } from "../lib/sheet-store.ts";
import { TrustBadge } from "./TrustBadge.tsx";
import { ProjectStatusTag } from "./ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "./BuilderTrustBadge.tsx";
import { DataFreshnessBadge } from "./DataFreshnessBadge.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  return !!value && value.trim().length > 0 && value !== "Not specified";
}

type ScoreBarProps = { label: string; value: number; color?: string };

function ScoreBar({ label, value, color = "var(--color-accent)" }: ScoreBarProps) {
  const pct = Math.round(value * 100);
  return (
    <div className="side-panel-score">
      <div className="side-panel-score-header">
        <span className="side-panel-score-label">{label}</span>
        <span className="side-panel-score-value">{pct}%</span>
      </div>
      <div className="side-panel-score-track">
        <div
          className="side-panel-score-fill"
          style={{ width: `${pct}%`, backgroundColor: color }}
        />
      </div>
    </div>
  );
}

function scoreColor(v: number): string {
  if (v >= 0.7) return "var(--color-positive)";
  if (v >= 0.4) return "var(--color-warning)";
  return "var(--color-negative)";
}

type Props = {
  propertyId: string;
  card: PropertyCardType;
  onClose: () => void;
  onSaveChange?: () => void;
};

export function PropertySidePanel({ propertyId, card, onClose, onSaveChange }: Props) {
  const [detail, setDetail] = useState<PropertyDetailResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(() => isOnSheet(propertyId));
  const [closing, setClosing] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const specs = [
    { value: card.bhk.toString(), label: "BHK" },
    hasKnownNumber(card.sqft) ? { value: card.sqft.toLocaleString("en-IN"), label: "sqft" } : null,
    hasKnownNumber(card.floor) && hasKnownNumber(card.total_floors)
      ? { value: `${card.floor}/${card.total_floors}`, label: "Floor" }
      : null,
    isKnownText(card.facing) ? { value: card.facing, label: "Facing" } : null,
  ].filter((spec): spec is { value: string; label: string } => spec !== null);

  const handleClose = useCallback(() => {
    setClosing(true);
    setTimeout(onClose, 250);
  }, [onClose]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setLoading(true);
      setDetail(null);
    });
    getProperty(propertyId)
      .then((data) => {
        if (!cancelled) setDetail(data);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [propertyId]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleClose]);

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) handleClose();
  };

  const handleSave = () => {
    setSaved(toggleSheetItem(propertyId));
    onSaveChange?.();
  };

  const p = detail?.property;
  const society = detail?.society;
  const area = detail?.area;
  const tradeoffs = detail?.tradeoffs;

  return (
    <div
      className={`side-panel-backdrop ${closing ? "side-panel-backdrop--closing" : ""}`}
      onClick={handleBackdropClick}
    >
      <div
        ref={panelRef}
        className={`side-panel ${closing ? "side-panel--closing" : ""}`}
        role="dialog"
        aria-label={`Property details: ${card.title}`}
      >
        {/* Header */}
        <div className="side-panel-header">
          <button className="side-panel-close" onClick={handleClose} aria-label="Close panel">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
          <span className="side-panel-header-label">Quick view</span>
        </div>

        {/* Hero image */}
        <div className="side-panel-hero">
          <ImageWithFallback
            src={card.hero_image}
            alt={card.title}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
          {card.transparency_tags.length > 0 && (
            <div className="side-panel-hero-tags">
              {card.transparency_tags.slice(0, 3).map((tag) => {
                const isSellerRegistered = tag === "seller-registered";
                const isVerificationPending = tag === "verification-pending";
                const tagStyle = isSellerRegistered
                  ? { background: "rgba(251, 191, 36, 0.85)", color: "#78350f" }
                  : isVerificationPending
                  ? { background: "rgba(156, 163, 175, 0.85)", color: "#fff" }
                  : undefined;
                return (
                  <span key={tag} className="side-panel-hero-tag" style={tagStyle}>
                    {tag.replace(/_/g, " ").replace(/-/g, " ")}
                  </span>
                );
              })}
            </div>
          )}
        </div>

        {/* Scrollable content */}
        <div className="side-panel-body">
          {/* Title + price */}
          <h2 className="side-panel-title">{card.title}</h2>
          <p className="side-panel-location">
            {card.society_name} &middot; {card.area}
          </p>

          {/* Trust badges */}
          <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", margin: "0.35rem 0 0.5rem" }}>
            <TrustBadge rootSource={card.root_source} compact />
            <ProjectStatusTag
              status={detail?.project_status}
              displayText={detail?.project_status_display}
              possessionStatus={card.possession_status}
            />
            {detail?.builder_trust?.delivery_display && (
              <BuilderTrustBadge
                deliveryDisplay={detail.builder_trust.delivery_display}
                deliveryRate={detail.builder_trust.delivery_rate}
                compact
              />
            )}
            <DataFreshnessBadge freshness={card.data_freshness ?? detail?.data_freshness} compact />
          </div>

          <div className="side-panel-price-row">
            <span className="side-panel-price">{formatPrice(card.price)}</span>
            {hasKnownNumber(card.price_per_sqft) && (
              <span className="side-panel-ppsqft">{card.price_per_sqft.toLocaleString("en-IN")} /sqft</span>
            )}
            {area && hasKnownNumber(card.price_per_sqft) && (
              <span
                className="side-panel-vs-median"
                style={{
                  color: card.price_per_sqft <= area.median_price_per_sqft
                    ? "var(--color-positive)" : "var(--color-negative)",
                }}
              >
                {card.price_per_sqft <= area.median_price_per_sqft
                  ? `${Math.round((1 - card.price_per_sqft / area.median_price_per_sqft) * 100)}% below median`
                  : `${Math.round((card.price_per_sqft / area.median_price_per_sqft - 1) * 100)}% above median`
                }
              </span>
            )}
          </div>

          {/* Specs row */}
          <div className="side-panel-specs">
            {specs.map((spec, index) => (
              <Fragment key={spec.label}>
                {index > 0 && <div className="side-panel-spec-divider" />}
                <div className="side-panel-spec">
                  <span className="side-panel-spec-value">{spec.value}</span>
                  <span className="side-panel-spec-label">{spec.label}</span>
                </div>
              </Fragment>
            ))}
          </div>

          {/* Loading state */}
          {loading && (
            <div className="side-panel-loading">
              <div className="side-panel-loading-bar" />
              <div className="side-panel-loading-bar" style={{ width: "60%" }} />
              <div className="side-panel-loading-bar" style={{ width: "80%" }} />
            </div>
          )}

          {/* Seller trust indicators */}
          {(card.seller_verified || (card.documents_provided && card.documents_provided.length > 0) || card.seller_completeness_pct != null) && (
            <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", flexWrap: "wrap", padding: "0.5rem 0", borderBottom: "1px solid var(--color-border, #e5e7eb)" }}>
              {card.seller_verified ? (
                <span style={{ display: "inline-flex", alignItems: "center", gap: "0.25rem", fontSize: "0.78rem", color: "#059669", fontWeight: 600 }}>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#059669" strokeWidth="2.5" strokeLinecap="round"><polyline points="20 6 9 17 4 12" /></svg>
                  Seller verified
                </span>
              ) : card.seller_completeness_pct != null ? (
                <span style={{ fontSize: "0.78rem", color: "#9ca3af" }}>
                  Verification pending
                </span>
              ) : null}
              {card.seller_completeness_pct != null && (
                <span style={{ fontSize: "0.75rem", color: "#6b7280", marginLeft: "auto" }}>
                  {card.seller_completeness_pct}% profile complete
                </span>
              )}
            </div>
          )}
          {(card.documents_provided && card.documents_provided.length > 0) && (
            <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", padding: "0.35rem 0" }}>
              {card.documents_provided.map((doc) => (
                <span
                  key={doc}
                  style={{
                    display: "inline-block",
                    fontSize: "0.7rem",
                    padding: "0.15rem 0.4rem",
                    borderRadius: "3px",
                    background: "#f0fdf4",
                    color: "#166534",
                    border: "1px solid #bbf7d0",
                    textTransform: "uppercase",
                    letterSpacing: "0.02em",
                  }}
                >
                  {doc.replace(/_/g, " ")}
                </span>
              ))}
            </div>
          )}

          {/* Description */}
          {p && (
            <p className="side-panel-description">{p.description_summary}</p>
          )}

          {/* Score bars */}
          {p && (
            <div className="side-panel-section">
              <h3 className="side-panel-section-title">Transparency scores</h3>
              <div className="side-panel-scores">
                <ScoreBar label="Society quality" value={p.society_quality_score} color={scoreColor(p.society_quality_score)} />
                <ScoreBar label="Builder quality" value={p.builder_quality_score} color={scoreColor(p.builder_quality_score)} />
                <ScoreBar label="Document completeness" value={p.document_completeness_score} color={scoreColor(p.document_completeness_score)} />
                {p.greenery_score != null && (
                  <ScoreBar label="Greenery" value={p.greenery_score} color={scoreColor(p.greenery_score)} />
                )}
                {p.resale_strength_score != null && (
                  <ScoreBar label="Resale strength" value={p.resale_strength_score} color={scoreColor(p.resale_strength_score)} />
                )}
              </div>

              {/* Risk indicator */}
              <div className="side-panel-risk">
                <span className="side-panel-risk-label">Litigation risk</span>
                <span
                  className="side-panel-risk-badge"
                  style={{
                    backgroundColor: p.litigation_risk <= 0.1 ? "var(--color-positive-bg)" : p.litigation_risk <= 0.3 ? "#fff7ed" : "var(--color-negative-bg)",
                    color: p.litigation_risk <= 0.1 ? "var(--color-positive)" : p.litigation_risk <= 0.3 ? "var(--color-warning)" : "var(--color-negative)",
                    borderColor: p.litigation_risk <= 0.1 ? "var(--color-positive-border)" : p.litigation_risk <= 0.3 ? "#fed7aa" : "var(--color-negative-border)",
                  }}
                >
                  {p.litigation_risk <= 0.1 ? "Low" : p.litigation_risk <= 0.3 ? "Moderate" : "High"}
                </span>
              </div>
            </div>
          )}

          {/* Tradeoffs */}
          {tradeoffs && (tradeoffs.strengths.length > 0 || tradeoffs.cautions.length > 0) && (
            <div className="side-panel-section">
              <h3 className="side-panel-section-title">At a glance</h3>
              {tradeoffs.strengths.length > 0 && (
                <ul className="side-panel-tradeoff-list">
                  {tradeoffs.strengths.slice(0, 3).map((s, i) => (
                    <li key={i} className="side-panel-tradeoff side-panel-tradeoff--positive">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-positive)" strokeWidth="2.5" strokeLinecap="round"><polyline points="20 6 9 17 4 12" /></svg>
                      {s}
                    </li>
                  ))}
                </ul>
              )}
              {tradeoffs.cautions.length > 0 && (
                <ul className="side-panel-tradeoff-list">
                  {tradeoffs.cautions.slice(0, 3).map((c, i) => (
                    <li key={i} className="side-panel-tradeoff side-panel-tradeoff--caution">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-warning)" strokeWidth="2.5" strokeLinecap="round"><circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" /></svg>
                      {c}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}

          {/* Society summary */}
          {society && (
            <div className="side-panel-section">
              <h3 className="side-panel-section-title">Society · {society.name}</h3>
              <p className="side-panel-society-review">{society.review_summary}</p>
              {society.common_positives.length > 0 && (
                <div className="side-panel-society-tags">
                  {society.common_positives.slice(0, 4).map((p) => (
                    <span key={p} className="tag tag-positive" style={{ fontSize: "0.72rem" }}>{p}</span>
                  ))}
                </div>
              )}
              {society.common_complaints.length > 0 && (
                <div className="side-panel-society-tags" style={{ marginTop: "0.35rem" }}>
                  {society.common_complaints.slice(0, 3).map((c) => (
                    <span key={c} className="tag" style={{ fontSize: "0.72rem", background: "var(--color-negative-bg)", color: "var(--color-negative)", border: "1px solid var(--color-negative-border)" }}>{c}</span>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Market signals */}
          {p && (
            <div className="side-panel-signals-grid">
              <div className="side-panel-signal-card">
                <span className="side-panel-signal-icon">
                  {p.possession_status === "ready" ? "\u2705" : "\u{1F3D7}\uFE0F"}
                </span>
                <span className="side-panel-signal-text">
                  {p.possession_status === "ready" ? "Ready to move" : "Under construction"}
                </span>
              </div>
              {hasKnownNumber(p.metro_distance_mins) && (
                <div className="side-panel-signal-card">
                  <span className="side-panel-signal-icon">{"\u{1F687}"}</span>
                  <span className="side-panel-signal-text">{p.metro_distance_mins} min to metro</span>
                </div>
              )}
              {hasKnownNumber(p.maintenance_cost_monthly) && (
                <div className="side-panel-signal-card">
                  <span className="side-panel-signal-icon">{"\u{1F4B0}"}</span>
                  <span className="side-panel-signal-text">
                    {"\u20B9"}{p.maintenance_cost_monthly.toLocaleString("en-IN")}/mo
                  </span>
                </div>
              )}
              {p.interest_level && (
                <div className="side-panel-signal-card">
                  <span className="side-panel-signal-icon">
                    {p.interest_level === "high" ? "\u{1F525}" : p.interest_level === "moderate" ? "\u{1F4CA}" : "\u{1F4AD}"}
                  </span>
                  <span className="side-panel-signal-text">
                    {p.interest_level.charAt(0).toUpperCase() + p.interest_level.slice(1)} interest
                  </span>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Sticky footer actions */}
        <div className="side-panel-footer">
          <button
            className={`side-panel-save-btn ${saved ? "side-panel-save-btn--saved" : ""}`}
            onClick={handleSave}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
            </svg>
            {saved ? "Saved" : "Save"}
          </button>
          <Link to={`/property/${propertyId}`} className="side-panel-full-btn">
            Full details
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
            </svg>
          </Link>
          <Link to={`/property/${propertyId}/plan`} className="side-panel-plan-btn">
            Plan
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </Link>
        </div>
      </div>
    </div>
  );
}
