import { Link } from "react-router-dom";
import type {
  PropertyCard,
  ProofFocus,
} from "../../lib/types.ts";
import { propertyDetailPath } from "../../lib/api.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";
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

function titleIncludesSociety(title: string, societyName: string): boolean {
  return title.toLocaleLowerCase("en-IN").includes(societyName.toLocaleLowerCase("en-IN"));
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
};

export function LivingEvidenceTile({
  property,
  onQuickView,
  proofFocus,
}: Props) {
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
    societyId: property.kg_entity_refs?.society_entity_id,
  });
  const cardImage = images[0] ?? property.hero_image ?? null;

  const societyKnown = isKnownText(property.society_name);
  const metaParts = [
    societyKnown && !titleIncludesSociety(property.title, property.society_name)
      ? property.society_name
      : null,
    property.area,
    titleIncludesBhk(property.title, property.bhk) ? null : `${property.bhk} BHK`,
    hasKnownNumber(property.sqft) ? `${property.sqft.toLocaleString("en-IN")} sqft` : null,
  ].filter((part): part is string => part !== null);

  const handleQuickView = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onQuickView?.(property.id);
  };

  return (
    <article className="catalog-card">
      <Link to={propertyDetailPath(property.id, proofFocus)} className="catalog-card__link">
        <div className="catalog-card__media">
          <ImageWithFallback
            src={cardImage}
            alt={property.title}
            className="catalog-card__image"
            loading="lazy"
          />
          <div className="catalog-card__actions" aria-label="Property actions">
            <SaveHeartButton propertyId={property.id} className="catalog-card__action catalog-card__save" />
            {onQuickView && (
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
            )}
          </div>
        </div>

        <div className="catalog-card__caption">
          <h3 className="catalog-card__title">{property.title}</h3>
          <p className="catalog-card__meta">{metaParts.join(" · ")}</p>
          <div className="catalog-card__foot">
            <span className="catalog-card__price">{formatPrice(property.price)}</span>
            {hasKnownNumber(property.google_rating) ? (
              <span className="catalog-card__rating">
                Google {property.google_rating.toFixed(1)}
                {hasKnownNumber(property.google_review_count) ? ` · ${property.google_review_count}` : ""}
              </span>
            ) : hasKnownNumber(property.price_per_sqft) ? (
              <span className="catalog-card__ppsf">
                {property.price_per_sqft.toLocaleString("en-IN")}/sqft
              </span>
            ) : null}
          </div>
        </div>
      </Link>
    </article>
  );
}
