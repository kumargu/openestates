import { useState, useEffect } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard as PropertyCardType, PropertyDetailResponse } from "../lib/types.ts";
import type { MatchResult } from "../lib/search.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";
import { getProperty } from "../lib/api.ts";
import { TrustBadge } from "./TrustBadge.tsx";
import { ProjectStatusTag } from "./ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "./BuilderTrustBadge.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

const LABEL_COLORS: Record<string, { bg: string; color: string; border: string }> = {
  "Strong match": { bg: "#edf7ed", color: "#2a7a2a", border: "#c8e6c8" },
  "Good match": { bg: "#f0f4ff", color: "#3b5998", border: "#c8d8f0" },
  "Value pick": { bg: "#fdf5e6", color: "#8a6d00", border: "#e8d8a0" },
  "Premium match": { bg: "#f5f0fa", color: "#6b3fa0", border: "#d8c8e8" },
};

export function PropertyCard({
  property,
  match,
  onShortlistChange,
}: {
  property: PropertyCardType;
  match?: MatchResult;
  onShortlistChange?: () => void;
}) {
  const [saved, setSaved] = useState(() => isShortlisted(property.id));
  const [expanded, setExpanded] = useState(false);
  const [detail, setDetail] = useState<PropertyDetailResponse | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  const handleSave = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const nowSaved = toggleShortlist(property.id);
    setSaved(nowSaved);
    onShortlistChange?.();
  };

  const handleExpand = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setExpanded(!expanded);
  };

  // Fetch detail when expanding for the first time
  useEffect(() => {
    if (expanded && !detail && !detailLoading) {
      setDetailLoading(true);
      getProperty(property.id)
        .then(setDetail)
        .catch(() => {})
        .finally(() => setDetailLoading(false));
    }
  }, [expanded, detail, detailLoading, property.id]);

  const labelStyle = match ? LABEL_COLORS[match.label] || LABEL_COLORS["Good match"] : null;

  return (
    <div className={`property-card-v2 ${expanded ? "property-card-v2--expanded" : ""}`}>
      {/* === Layer A: Always visible === */}
      <Link to={`/property/${property.id}`} className="property-card-v2-link">
        <div className="property-card-v2-image">
          <ImageWithFallback
            src={property.hero_image}
            alt={property.title}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />

          {/* Save button */}
          <button
            onClick={handleSave}
            className={`property-card-save ${saved ? "property-card-save--saved" : ""}`}
            title={saved ? "Remove from shortlist" : "Save to shortlist"}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
            </svg>
            <span className="property-card-save-label">
              {saved ? "Saved" : "Save"}
            </span>
          </button>

          {/* Match label overlay */}
          {match && labelStyle && (
            <span
              className="property-card-match"
              style={{
                background: labelStyle.bg,
                color: labelStyle.color,
                border: `1px solid ${labelStyle.border}`,
              }}
            >
              {match.label}
            </span>
          )}
        </div>

        <div className="property-card-v2-body">
          <h3 className="property-card-v2-title">{property.title}</h3>
          <p className="property-card-v2-location">
            {property.society_name} &middot; {property.area}
          </p>

          <div className="property-card-v2-price-row">
            <span className="property-card-v2-price">{formatPrice(property.price)}</span>
            <span className="property-card-v2-ppsqft">
              {property.price_per_sqft.toLocaleString("en-IN")} /sqft
            </span>
          </div>

          <div className="property-card-v2-specs">
            <span>{property.bhk} BHK</span>
            <span>&middot;</span>
            <span>{property.sqft} sqft</span>
            <span>&middot;</span>
            <span>{property.facing}</span>
            <span>&middot;</span>
            <span>Floor {property.floor}/{property.total_floors}</span>
          </div>

          {/* Why this property */}
          {match && (
            <p className="property-card-v2-reason">{match.reason}</p>
          )}

          {/* Compact signals row */}
          <div className="property-card-v2-signals">
            <ProjectStatusTag
              status={property.project_status}
              displayText={property.project_status_display}
              possessionStatus={property.possession_status}
            />
            <span className="property-signal">
              {property.metro_distance_mins} min to metro
            </span>
            <span className="property-signal">
              {property.builder_name}
            </span>
            <BuilderTrustBadge deliveryDisplay={property.builder_delivery_display} compact />
            <TrustBadge rootSource={property.root_source} compact />
          </div>

          {/* Seller trust indicators */}
          {(property.seller_verified || (property.documents_provided && property.documents_provided.length > 0) || property.seller_completeness_pct) && (
            <div className="property-card-v2-trust" style={{ display: "flex", alignItems: "center", gap: "0.4rem", flexWrap: "wrap", marginTop: "0.25rem" }}>
              {property.seller_verified && (
                <span style={{ display: "inline-flex", alignItems: "center", gap: "0.2rem", fontSize: "0.72rem", color: "#059669", fontWeight: 500 }}>
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#059669" strokeWidth="2.5" strokeLinecap="round"><polyline points="20 6 9 17 4 12" /></svg>
                  Verified seller
                </span>
              )}
              {property.documents_provided && property.documents_provided.length > 0 && (
                property.documents_provided.map((doc) => (
                  <span
                    key={doc}
                    style={{
                      display: "inline-block",
                      fontSize: "0.68rem",
                      padding: "0.1rem 0.35rem",
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
                ))
              )}
              {property.seller_completeness_pct != null && !property.seller_verified && (
                <span style={{ fontSize: "0.7rem", color: "#9ca3af" }}>
                  {property.seller_completeness_pct}% profile
                </span>
              )}
            </div>
          )}

          {/* Tags */}
          {property.transparency_tags.length > 0 && (
            <div className="property-card-v2-tags">
              {property.transparency_tags.map((tag) => {
                const isSellerRegistered = tag === "seller-registered";
                const isVerificationPending = tag === "verification-pending";
                const tagStyle = isSellerRegistered
                  ? {
                      background: "#fffbeb",
                      color: "#92400e",
                      border: "1px solid #fcd34d",
                      fontSize: "0.72rem",
                    }
                  : isVerificationPending
                  ? {
                      background: "#f9fafb",
                      color: "#6b7280",
                      border: "1px solid #e5e7eb",
                      fontSize: "0.72rem",
                    }
                  : undefined;
                return (
                  <span
                    key={tag}
                    className={isSellerRegistered || isVerificationPending ? "tag" : "tag tag-positive"}
                    style={tagStyle}
                  >
                    {tag.replace(/_/g, " ").replace(/-/g, " ")}
                  </span>
                );
              })}
            </div>
          )}
        </div>
      </Link>

      {/* Quick-view toggle */}
      <button onClick={handleExpand} className="property-card-expand">
        <svg
          width="14" height="14" viewBox="0 0 24 24" fill="none"
          stroke="currentColor" strokeWidth="2" strokeLinecap="round"
          style={{ transform: expanded ? "rotate(180deg)" : "none", transition: "transform 0.2s ease" }}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
        {expanded ? "Less" : "Quick view"}
      </button>

      {/* === Layer B: Expandable quick-view === */}
      {expanded && (
        <div className="property-card-quickview">
          {detailLoading && (
            <p style={{ color: "var(--color-text-muted)", fontSize: "0.85rem" }}>Loading details...</p>
          )}

          {!detailLoading && !detail && (
            <p className="quickview-summary">{property.description_summary}</p>
          )}

          {detail && (
            <>
              <p className="quickview-summary">{detail.property.description_summary}</p>

              {/* Price context */}
              {detail.area && (
                <div className="quickview-row">
                  <span className="quickview-label">Price vs area median</span>
                  <span className="quickview-value" style={{
                    color: property.price_per_sqft <= detail.area.median_price_per_sqft
                      ? "var(--color-positive)" : "var(--color-text)",
                    fontWeight: 600,
                  }}>
                    {property.price_per_sqft <= detail.area.median_price_per_sqft
                      ? `${Math.round((1 - property.price_per_sqft / detail.area.median_price_per_sqft) * 100)}% below median`
                      : `${Math.round((property.price_per_sqft / detail.area.median_price_per_sqft - 1) * 100)}% above median`
                    }
                  </span>
                </div>
              )}

              {/* Key signals grid */}
              <div className="quickview-grid">
                <div className="quickview-stat">
                  <span className="quickview-stat-label">Society</span>
                  <span className="quickview-stat-value">
                    {detail.property.society_quality_score >= 0.7 ? "Strong" : detail.property.society_quality_score >= 0.5 ? "Decent" : "Weak"}
                  </span>
                </div>
                <div className="quickview-stat">
                  <span className="quickview-stat-label">Docs</span>
                  <span className="quickview-stat-value">
                    {detail.property.document_completeness_score >= 0.8 ? "Complete" : "Partial"}
                  </span>
                </div>
                <div className="quickview-stat">
                  <span className="quickview-stat-label">Risk</span>
                  <span className="quickview-stat-value" style={{
                    color: detail.property.litigation_risk <= 0.1 ? "var(--color-positive)" : "var(--color-warning)",
                  }}>
                    {detail.property.litigation_risk <= 0.1 ? "Low" : detail.property.litigation_risk <= 0.3 ? "Moderate" : "High"}
                  </span>
                </div>
                <div className="quickview-stat">
                  <span className="quickview-stat-label">Market</span>
                  <span className="quickview-stat-value">
                    {detail.property.interest_level || "\u2014"}
                  </span>
                </div>
              </div>

              {/* Society summary */}
              {detail.society && (
                <div className="quickview-section">
                  <span className="quickview-section-title">Society</span>
                  <p className="quickview-section-text">{detail.society.review_summary}</p>
                </div>
              )}

              {/* Area summary */}
              {detail.area && (
                <div className="quickview-section">
                  <span className="quickview-section-title">Area</span>
                  <p className="quickview-section-text">{detail.area.livability_summary}</p>
                </div>
              )}
            </>
          )}

          <div className="quickview-actions">
            <button onClick={handleSave} className={`btn ${saved ? "btn-outline" : "btn-primary"}`} style={{ fontSize: "0.82rem" }}>
              {saved ? "Saved to shortlist" : "Save to shortlist"}
            </button>
            <Link to={`/property/${property.id}`} className="btn btn-outline" style={{ fontSize: "0.82rem", textDecoration: "none" }}>
              Full details
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
