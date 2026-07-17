import { useState } from "react";
import { Link } from "react-router-dom";
import type {
  ConfidenceScore,
  PropertyCard,
  PropertyEvidenceResponse,
} from "../../lib/types.ts";
import type { MatchResult } from "../../lib/search.ts";
import {
  summarizeEvidence,
  topEvidenceGlance,
} from "../../lib/evidence.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { isOnSheet, toggleSheetItem } from "../../lib/sheet-store.ts";
import { usePropertySceneImages } from "../../hooks/usePropertySceneImages.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  if (!value) return false;
  const lowered = value.trim().toLowerCase();
  return lowered.length > 0 && lowered !== "not specified" && lowered !== "unknown" && lowered !== "n/a";
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
  evidence,
  explanationBlock,
  onQuickView,
  onSaveChange,
}: Props) {
  const [onSheet, setOnSheet] = useState(() => isOnSheet(property.id));
  const summary = summarizeEvidence(evidence);
  const glances = topEvidenceGlance(evidence, 1);
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
    societyId: property.kg_entity_refs?.society_entity_id,
  });
  const cardImage = images[0] ?? property.hero_image ?? null;

  const metaParts = [
    isKnownText(property.society_name) ? property.society_name : null,
    property.area,
    `${property.bhk} BHK`,
    hasKnownNumber(property.sqft) ? `${property.sqft.toLocaleString("en-IN")} sqft` : null,
  ].filter((part): part is string => part !== null);

  const whyLine = match?.reason || glances[0] || null;

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

  return (
    <article className="catalog-card">
      <Link to={`/property/${property.id}`} className="catalog-card__link">
        <div className="catalog-card__media">
          <ImageWithFallback
            src={cardImage}
            alt={property.title}
            className="catalog-card__image"
            loading="lazy"
          />
          {match && (
            <span className="catalog-card__kicker">{match.label}</span>
          )}
          <div className="catalog-card__actions" aria-label="Property actions">
            <button
              type="button"
              onClick={handleSave}
              className={`catalog-card__action${onSheet ? " catalog-card__action--saved" : ""}`}
              aria-label={onSheet ? "Remove from saved" : "Save property"}
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill={onSheet ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
              </svg>
            </button>
            <button
              type="button"
              onClick={handleQuickView}
              className="catalog-card__action"
              aria-label="Quick view"
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="16" x2="12" y2="12" />
                <line x1="12" y1="8" x2="12.01" y2="8" />
              </svg>
            </button>
          </div>
        </div>

        <div className="catalog-card__caption">
          <h3 className="catalog-card__title">{property.title}</h3>
          <p className="catalog-card__meta">{metaParts.join(" · ")}</p>
          <div className="catalog-card__foot">
            <span className="catalog-card__price">{formatPrice(property.price)}</span>
            {hasKnownNumber(property.price_per_sqft) && (
              <span className="catalog-card__ppsf">
                {property.price_per_sqft.toLocaleString("en-IN")}/sqft
              </span>
            )}
          </div>
          {whyLine && <p className="catalog-card__why">{whyLine}</p>}
          {summary && (
            <p className="catalog-card__proof">
              {summary.factCount} facts · {summary.confidencePct}% confidence
            </p>
          )}
          {explanationBlock && (
            <div className="catalog-card__explain">{explanationBlock}</div>
          )}
        </div>
      </Link>
    </article>
  );
}
