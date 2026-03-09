import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard as PropertyCardType } from "../lib/types.ts";
import type { MatchResult } from "../lib/search.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
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

  const handleSave = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const nowSaved = toggleShortlist(property.id);
    setSaved(nowSaved);
    onShortlistChange?.();
  };

  const labelStyle = match ? LABEL_COLORS[match.label] || LABEL_COLORS["Good match"] : null;

  return (
    <Link to={`/property/${property.id}`} className="property-card" data-testid="property-card-link" style={{ position: "relative" }}>
      <ImageWithFallback
        src={property.hero_image}
        alt={property.title}
        style={{ width: "100%", height: "180px" }}
      />

      {/* Save button */}
      <button
        onClick={handleSave}
        style={{
          position: "absolute",
          top: "0.75rem",
          right: "0.75rem",
          width: "32px",
          height: "32px",
          borderRadius: "50%",
          border: "none",
          background: saved ? "var(--color-accent)" : "rgba(255,255,255,0.9)",
          color: saved ? "#fff" : "var(--color-text-muted)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backdropFilter: "blur(8px)",
          transition: "all 0.2s ease",
          boxShadow: "0 2px 8px rgba(0,0,0,0.1)",
        }}
        title={saved ? "Remove from shortlist" : "Save to shortlist"}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
        </svg>
      </button>

      <div style={{ padding: "1.25rem" }}>
        {/* Match label */}
        {match && labelStyle && (
          <div style={{ marginBottom: "0.6rem" }}>
            <span style={{
              fontSize: "0.7rem",
              fontWeight: 600,
              padding: "0.2rem 0.6rem",
              borderRadius: "var(--radius-xl)",
              background: labelStyle.bg,
              color: labelStyle.color,
              border: `1px solid ${labelStyle.border}`,
              textTransform: "uppercase",
              letterSpacing: "0.04em",
            }}>
              {match.label}
            </span>
          </div>
        )}

        <h3 style={{
          margin: "0 0 0.25rem",
          fontSize: "1rem",
          fontWeight: 600,
          letterSpacing: "-0.01em",
        }}>
          {property.title}
        </h3>
        <p style={{ margin: "0 0 0.6rem", color: "var(--color-text-secondary)", fontSize: "0.85rem" }}>
          {property.society_name} &middot; {property.area}
        </p>

        <div style={{ display: "flex", alignItems: "baseline", gap: "0.5rem", marginBottom: "0.4rem" }}>
          <span style={{ fontSize: "1.2rem", fontWeight: 700 }}>
            {formatPrice(property.price)}
          </span>
          <span style={{ color: "var(--color-text-muted)", fontSize: "0.8rem" }}>
            {property.price_per_sqft.toLocaleString("en-IN")} /sqft
          </span>
        </div>

        <p style={{ margin: "0 0 0.6rem", color: "var(--color-text-muted)", fontSize: "0.8rem" }}>
          {property.bhk} BHK &middot; {property.sqft} sqft
        </p>

        {/* Why this property */}
        {match && (
          <p style={{
            margin: "0 0 0.6rem",
            fontSize: "0.8rem",
            color: "var(--color-text-secondary)",
            lineHeight: 1.4,
            fontStyle: "italic",
          }}>
            {match.reason}
          </p>
        )}

        {property.transparency_tags.length > 0 && (
          <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
            {property.transparency_tags.map((tag) => (
              <span key={tag} className="tag tag-positive">
                {tag.replace(/_/g, " ")}
              </span>
            ))}
          </div>
        )}
      </div>
    </Link>
  );
}
