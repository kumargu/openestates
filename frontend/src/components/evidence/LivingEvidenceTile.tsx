import { useState } from "react";
import { Link } from "react-router-dom";
import type {
  ConfidenceScore,
  PropertyCard,
  PropertyEvidenceResponse,
} from "../../lib/types.ts";
import type { MatchResult } from "../../lib/search.ts";
import {
  evidenceHeatClass,
  summarizeEvidence,
  topEvidenceGlance,
  type EvidenceSummary,
} from "../../lib/evidence.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { isOnSheet, toggleSheetItem } from "../../lib/sheet-store.ts";
import { ProjectStatusTag } from "../ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "../BuilderTrustBadge.tsx";
import { TrustBadge } from "../TrustBadge.tsx";
import { DataFreshnessBadge } from "../DataFreshnessBadge.tsx";
import { ConfidenceMeter } from "../ConfidenceMeter.tsx";

const LABEL_COLORS: Record<string, { bg: string; color: string; border: string }> = {
  "Strong match": { bg: "#edf7ed", color: "#2a7a2a", border: "#c8e6c8" },
  "Good match": { bg: "#f0f4ff", color: "#3b5998", border: "#c8d8f0" },
  "Value pick": { bg: "#fdf5e6", color: "#8a6d00", border: "#e8d8a0" },
  "Premium match": { bg: "#f5f0fa", color: "#6b3fa0", border: "#d8c8e8" },
  "Partial match": { bg: "#f8f6f2", color: "#6f6258", border: "rgba(0,0,0,0.08)" },
};

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  return !!value && value.trim().length > 0 && value !== "Not specified";
}

type Props = {
  property: PropertyCard;
  match?: MatchResult;
  confidenceScore?: ConfidenceScore;
  evidence?: PropertyEvidenceResponse;
  decisionRead?: string;
  explanationBlock?: React.ReactNode;
  onQuickView?: (id: string) => void;
  onSaveChange?: () => void;
};

export function LivingEvidenceTile({
  property,
  match,
  confidenceScore,
  evidence,
  decisionRead,
  explanationBlock,
  onQuickView,
  onSaveChange,
}: Props) {
  const [onSheet, setOnSheet] = useState(() => isOnSheet(property.id));
  const summary: EvidenceSummary | null = summarizeEvidence(evidence);
  const glances = topEvidenceGlance(evidence, 1);
  const heatClass = summary ? evidenceHeatClass(summary.heat) : "evidence-heat--sparse";

  const specs = [
    `${property.bhk} BHK`,
    hasKnownNumber(property.sqft) ? `${property.sqft.toLocaleString("en-IN")} sqft` : null,
    isKnownText(property.facing) ? property.facing : null,
    hasKnownNumber(property.floor) && hasKnownNumber(property.total_floors)
      ? `Floor ${property.floor}/${property.total_floors}`
      : null,
  ].filter((spec): spec is string => spec !== null);

  const handleSave = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOnSheet(toggleSheetItem(property.id));
    onSaveChange?.();
  };

  const handleQuickView = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onQuickView?.(property.id);
  };

  const labelStyle = match ? LABEL_COLORS[match.label] || LABEL_COLORS["Good match"] : null;

  return (
    <div className={`card-a living-evidence-tile ${heatClass}`}>
      <Link to={`/property/${property.id}`} className="card-a-link">
        <div className="card-a-image">
          <ImageWithFallback
            src={property.hero_image}
            alt={property.title}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
          {match && labelStyle && (
            <span
              className="card-a-match"
              style={{ background: labelStyle.bg, color: labelStyle.color, border: `1px solid ${labelStyle.border}` }}
            >
              {match.label}
            </span>
          )}
        </div>

        <div className="card-a-body">
          {(decisionRead || summary) && (
            <div className="living-evidence-tile__glance">
              {decisionRead && (
                <span className="living-evidence-tile__read">{decisionRead}</span>
              )}
              {summary && (
                <span className="living-evidence-tile__proof">
                  {summary.factCount} facts
                  {summary.gapCount > 0 ? ` · ${summary.gapCount} gaps` : ""}
                  {` · ${summary.confidencePct}%`}
                </span>
              )}
            </div>
          )}

          <h3 className="card-a-title">{property.title}</h3>
          <p className="card-a-location">
            {property.society_name ? `${property.society_name} · ` : ""}{property.area}
          </p>

          <div className="card-a-price-row">
            <span className="card-a-price">{formatPrice(property.price)}</span>
            {hasKnownNumber(property.price_per_sqft) && (
              <span className="card-a-ppsqft">{property.price_per_sqft.toLocaleString("en-IN")} /sqft</span>
            )}
          </div>

          <div className="card-a-specs">
            {specs.map((spec, index) => (
              <span key={spec}>
                {index > 0 && <span>&middot; </span>}
                {spec}
              </span>
            ))}
          </div>

          {match && <p className="card-a-reason">{match.reason}</p>}

          {glances.length > 0 && (
            <p className="living-evidence-tile__evidence-line">{glances[0]}</p>
          )}

          {explanationBlock}

          <div className="card-a-signals">
            <ProjectStatusTag
              status={property.project_status}
              displayText={property.project_status_display}
              possessionStatus={property.possession_status}
            />
            {hasKnownNumber(property.metro_distance_mins) && (
              <span className="property-signal">{property.metro_distance_mins} min to metro</span>
            )}
            {isKnownText(property.builder_name) && (
              <span className="property-signal">{property.builder_name}</span>
            )}
            <BuilderTrustBadge deliveryDisplay={property.builder_delivery_display} compact />
            <TrustBadge rootSource={property.root_source} compact />
            <DataFreshnessBadge freshness={property.data_freshness} compact />
            <ConfidenceMeter confidence={confidenceScore} compact />
          </div>

          {property.kg_entity_refs.society_entity_id && (
            <div className="living-evidence-tile__refs" title="Knowledge graph handles for dynamic evidence">
              KG linked
            </div>
          )}
        </div>
      </Link>

      <div className="card-a-actions">
        <button onClick={handleSave} className={`card-a-save-btn ${onSheet ? "card-a-save-btn--saved" : ""}`}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill={onSheet ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
          </svg>
          {onSheet ? "Saved" : "Save"}
        </button>
        <button className="card-a-detail-btn" onClick={handleQuickView}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
          </svg>
          <span className="card-a-detail-btn-label">Quick view</span>
        </button>
      </div>
    </div>
  );
}
