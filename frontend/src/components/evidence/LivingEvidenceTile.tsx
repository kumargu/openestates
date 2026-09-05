import { Link } from "react-router-dom";
import type { PropertyCard, ProofFocus } from "../../lib/types.ts";
import { propertyDetailPath } from "../../lib/api.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";
import { usePropertySceneImages } from "../../hooks/usePropertySceneImages.ts";
import { formatListingPrice } from "../../lib/listing-price.ts";

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
  const normalizedTitle = title.toLocaleLowerCase("en-IN");
  return [societyName, societyName.split(",")[0] ?? ""]
    .map((name) => name.trim().toLocaleLowerCase("en-IN"))
    .filter((name) => name.length > 3)
    .some((name) => normalizedTitle.includes(name));
}

function titleIncludesBhk(title: string, bhk: number): boolean {
  return new RegExp(`\\b${bhk}\\s*bhk\\b`, "i").test(title);
}

function landingTitle(title: string): string {
  return title.replace(/^\s*\d+\s*BHK\s+in\s+/i, "").trim() || title;
}

function areaWithoutRepeatedIdentity(title: string, area: string): string | null {
  const parts = area
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((part, index, all) => (
      index === 0
      || part.toLocaleLowerCase("en-IN") !== all[index - 1]?.toLocaleLowerCase("en-IN")
    ));
  const normalizedTitle = title.toLocaleLowerCase("en-IN");
  while (
    parts.length > 0
    && parts[0]
    && normalizedTitle.includes(parts[0].toLocaleLowerCase("en-IN"))
  ) {
    parts.shift();
  }
  return parts.join(", ") || null;
}

type Props = {
  property: PropertyCard;
  /** Landing/browse surfaces — same card shell, minimal meta. */
  variant?: "default" | "browse";
  proofFocus?: ProofFocus;
  discoveryContextId?: string | null;
  discoveryQueryFingerprint?: string | null;
  matchLabels?: string[];
  /** Keep shortlist entry points explicit instead of enabling them on every card surface. */
  allowSave?: boolean;
  /** Landing rails may progressively disclose one short reason without becoming detail cards. */
  previewActive?: boolean;
  previewReason?: string | null;
  previewSignals?: string[];
  spatial?: boolean;
};

export function LivingEvidenceTile({
  property,
  variant = "default",
  proofFocus,
  discoveryContextId,
  discoveryQueryFingerprint,
  matchLabels = [],
  allowSave = false,
  previewActive,
  previewReason,
  previewSignals = [],
  spatial = false,
}: Props) {
  const { images } = usePropertySceneImages({
    heroImage: property.hero_image,
  });
  const cardImage = images[0] ?? property.hero_image ?? null;

  const hasPreview = previewActive !== undefined;
  const displayTitle = hasPreview ? landingTitle(property.title) : property.title;
  const societyKnown = isKnownText(property.society_name);
  const displayArea = areaWithoutRepeatedIdentity(displayTitle, property.area);
  const metaParts = [
    societyKnown && !titleIncludesSociety(displayTitle, property.society_name)
      ? property.society_name
      : null,
    displayArea,
    !hasKnownNumber(property.bhk) || titleIncludesBhk(displayTitle, property.bhk)
      ? null
      : `${property.bhk} BHK`,
    hasKnownNumber(property.sqft)
      ? `${property.sqft.toLocaleString("en-IN")} sqft`
      : null,
  ].filter((part): part is string => part !== null);
  const fitLabel = hasPreview ? matchLabels[0] : null;
  const visibleMatchLabels = hasPreview ? [] : matchLabels;

  return (
    <article
      className={[
        "catalog-card",
        variant === "browse" ? "catalog-card--browse" : "",
        hasPreview ? "catalog-card--preview" : "",
        spatial ? "catalog-card--spatial" : "",
        previewActive ? "is-preview-active" : "",
      ].filter(Boolean).join(" ")}
    >
      <div className="catalog-card__media">
        <ImageWithFallback
          src={cardImage}
          alt=""
          className="catalog-card__image"
          loading="lazy"
          fetchPriority="low"
        />
        {allowSave ? (
          <div className="catalog-card__actions" role="group" aria-label="Property actions">
            <SaveHeartButton
              propertyId={property.id}
              propertyName={displayTitle}
              className="catalog-card__action catalog-card__save"
            />
          </div>
        ) : null}
      </div>
      <Link
        to={propertyDetailPath(
          property.id,
          proofFocus,
          discoveryContextId,
          discoveryQueryFingerprint,
        )}
        className="catalog-card__link"
      >
        <div className="catalog-card__caption">
          <h3 className="catalog-card__title">{displayTitle}</h3>
          <p className="catalog-card__meta">{metaParts.join(" · ")}</p>
          <div className="catalog-card__foot">
            <span className="catalog-card__price">
              {formatListingPrice(property)}
            </span>
            {(!hasPreview || !fitLabel) && hasKnownNumber(property.google_rating) ? (
              <span className="catalog-card__rating">
                Google {property.google_rating.toFixed(1)}
                {hasKnownNumber(property.google_review_count)
                  ? ` · ${property.google_review_count}`
                  : ""}
              </span>
            ) : (!hasPreview || !fitLabel) && hasKnownNumber(property.price_per_sqft) ? (
              <span className="catalog-card__ppsf">
                {property.price_per_sqft.toLocaleString("en-IN")}/sqft
              </span>
            ) : null}
          </div>
          {fitLabel ? (
            <p className="catalog-card__fit">{fitLabel}</p>
          ) : null}
          {visibleMatchLabels.length > 0 && (
            <div className="catalog-card__signals" aria-label="Search match">
              {visibleMatchLabels.slice(0, 2).map((label) => (
                <span key={label} className="catalog-card__signal">
                  {label}
                </span>
              ))}
            </div>
          )}
          {previewActive ? (
            <div className="catalog-card__peek">
              {previewReason ? (
                <p className="catalog-card__why">{previewReason}</p>
              ) : null}
              {previewSignals.length > 0 ? (
                <p className="catalog-card__preview-signals">
                  {previewSignals.slice(0, 3).map((signal) => (
                    <span key={signal}>{signal}</span>
                  ))}
                </p>
              ) : null}
              <span className="catalog-card__open">
                Open home <span aria-hidden="true">→</span>
              </span>
            </div>
          ) : null}
        </div>
      </Link>
    </article>
  );
}
