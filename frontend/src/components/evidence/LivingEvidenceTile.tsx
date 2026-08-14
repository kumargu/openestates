import { Link } from "react-router-dom";
import type {
  BuyerProofProjection,
  PropertyCard,
  ProofFocus,
} from "../../lib/types.ts";
import { propertyDetailPath } from "../../lib/api.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";
import { usePropertySceneImages } from "../../hooks/usePropertySceneImages.ts";
import {
  buyerProofCoverageLabel,
  buyerProofReceiptLabel,
} from "../../lib/buyerProof.ts";

function formatPrice(price: number): string {
  if (!hasKnownNumber(price)) return "Price unavailable";
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
  return (
    lowered.length > 0 &&
    lowered !== "not specified" &&
    lowered !== "unknown" &&
    lowered !== "n/a"
  );
}

function titleIncludesSociety(title: string, societyName: string): boolean {
  return title
    .toLocaleLowerCase("en-IN")
    .includes(societyName.toLocaleLowerCase("en-IN"));
}

function titleIncludesBhk(title: string, bhk: number): boolean {
  return new RegExp(`\\b${bhk}\\s*bhk\\b`, "i").test(title);
}

type Props = {
  property: PropertyCard;
  onQuickView?: (id: string) => void;
  /** Landing/browse surfaces — same card shell, minimal meta. */
  variant?: "default" | "browse";
  proofFocus?: ProofFocus;
  buyerProof?: BuyerProofProjection;
  /** Landing previews keep one concrete receipt and defer coverage gaps to search. */
  proofDensity?: "full" | "receipt";
  matchLabels?: string[];
  /** Keep shortlist entry points explicit instead of enabling them on every card surface. */
  allowSave?: boolean;
};

export function LivingEvidenceTile({
  property,
  onQuickView,
  variant = "default",
  proofFocus,
  buyerProof,
  proofDensity = "full",
  matchLabels = [],
  allowSave = false,
}: Props) {
  const { media } = usePropertySceneImages({
    media: property.hero_media ? [property.hero_media] : [],
  });
  const cardImage = media[0]?.url ?? null;

  const societyKnown = isKnownText(property.society_name);
  const metaParts = [
    societyKnown && !titleIncludesSociety(property.title, property.society_name)
      ? property.society_name
      : null,
    property.area,
    titleIncludesBhk(property.title, property.bhk)
      ? null
      : `${property.bhk} BHK`,
    hasKnownNumber(property.sqft)
      ? `${property.sqft.toLocaleString("en-IN")} sqft`
      : null,
  ].filter((part): part is string => part !== null);

  const handleQuickView = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onQuickView?.(property.id);
  };
  const receipt = buyerProof?.receipt;
  const coverageGap = buyerProof?.coverage_gap;

  return (
    <article
      className={`catalog-card${variant === "browse" ? " catalog-card--browse" : ""}`}
    >
      <div className="catalog-card__media">
        <ImageWithFallback
          src={cardImage}
          alt=""
          className="catalog-card__image"
          loading="lazy"
          fetchPriority="low"
        />
        {allowSave || onQuickView ? (
          <div className="catalog-card__actions" role="group" aria-label="Property actions">
            {allowSave ? (
              <SaveHeartButton
                propertyId={property.id}
                itemLabel={property.title}
                className="catalog-card__action catalog-card__save"
              />
            ) : null}
            {onQuickView ? (
              <button
                type="button"
                onClick={handleQuickView}
                className="catalog-card__action"
                aria-label="Quick view"
              >
                <svg
                  width="15"
                  height="15"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  <circle cx="12" cy="12" r="10" />
                  <line x1="12" y1="16" x2="12" y2="12" />
                  <line x1="12" y1="8" x2="12.01" y2="8" />
                </svg>
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
      <Link
        to={propertyDetailPath(property.id, proofFocus)}
        className="catalog-card__link"
      >
        <div className="catalog-card__caption">
          <h3 className="catalog-card__title">{property.title}</h3>
          <p className="catalog-card__meta">{metaParts.join(" · ")}</p>
          <div className="catalog-card__foot">
            <span className="catalog-card__price">
              {formatPrice(property.price)}
            </span>
            {hasKnownNumber(property.google_rating) ? (
              <span className="catalog-card__rating">
                Google {property.google_rating.toFixed(1)}
                {hasKnownNumber(property.google_review_count)
                  ? ` · ${property.google_review_count}`
                  : ""}
              </span>
            ) : hasKnownNumber(property.price_per_sqft) ? (
              <span className="catalog-card__ppsf">
                {property.price_per_sqft.toLocaleString("en-IN")}/sqft
              </span>
            ) : null}
          </div>
          {receipt ? (
            <p className={`catalog-card__receipt${proofDensity === "receipt" ? " catalog-card__receipt--compact" : ""}`}>
              {proofDensity === "full" ? <span>Why it fits</span> : null}
              {buyerProofReceiptLabel(receipt)}
            </p>
          ) : matchLabels.length > 0 ? (
            <div className="catalog-card__signals" aria-label="Search match">
              {matchLabels.slice(0, 2).map((label) => (
                <span key={label} className="catalog-card__signal">
                  {label}
                </span>
              ))}
            </div>
          ) : null}
          {coverageGap && proofDensity === "full" ? (
            <p className="catalog-card__coverage-gap">
              {buyerProofCoverageLabel(coverageGap)}
            </p>
          ) : null}
        </div>
      </Link>
    </article>
  );
}
