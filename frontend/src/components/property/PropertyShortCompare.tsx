import { Link } from "react-router-dom";
import uiSurfacesConfig from "../../../../app/config/dag/ui_surfaces.json";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import type { StoryComparison } from "../../lib/propertyStory.ts";
import {
  hrefWithSearchSpan,
  propertyHrefWithSearchSpan,
  searchSpanReferenceForTarget,
} from "../../lib/navigationContext.ts";
import { useSearchSpan } from "../workspace/SearchSpanContext.ts";
import "../../styles/property-fact-decks.css";

type Props = {
  homes: StoryComparison[];
  compareHref?: string;
};

function formatPrice(price?: number): string | undefined {
  if (!price || !Number.isFinite(price) || price <= 0) return undefined;
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

type ComparisonValueKey =
  | "price"
  | "bhk"
  | "sizeLabel"
  | "status"
  | "googleRating";
type ComparisonFormat = "inr_short" | "bhk" | "rating" | "text";

type ComparisonDimension = {
  key: string;
  label: string;
  valueKey: ComparisonValueKey;
  format: ComparisonFormat;
};

type UiSurfacesConfig = {
  surfaces: Array<{
    id: string;
    title?: string;
    comparisonDimensions?: ComparisonDimension[];
  }>;
};

const COMPARISON_SURFACE = (uiSurfacesConfig as UiSurfacesConfig).surfaces.find(
  (surface) => surface.id === "property_short_compare",
);
const COMPARISON_DIMENSIONS = COMPARISON_SURFACE?.comparisonDimensions ?? [];

function dimensionValue(
  home: StoryComparison,
  dimension: ComparisonDimension,
): string | undefined {
  const value = home[dimension.valueKey];
  if (dimension.format === "inr_short") {
    return typeof value === "number" ? formatPrice(value) : undefined;
  }
  if (dimension.format === "bhk") {
    return typeof value === "number" && value > 0
      ? `${value} BHK`
      : undefined;
  }
  if (dimension.format === "rating") {
    return typeof value === "number" && value > 0
      ? `Google ${value.toFixed(1)}`
      : undefined;
  }
  return typeof value === "string" && value.trim() ? value : undefined;
}

export function PropertyShortCompare({ homes, compareHref }: Props) {
  const searchSpan = useSearchSpan();
  if (homes.length !== 3 || !compareHref) return null;
  const dimensions = COMPARISON_DIMENSIONS.filter((dimension) =>
    homes.every((home) => Boolean(dimensionValue(home, dimension))));
  if (dimensions.length === 0) return null;
  const showImages = homes.every((home) => Boolean(home.heroImage));

  return (
    <section
      id="short-compare"
      className="property-fact-deck property-short-compare"
      aria-labelledby="property-short-compare-title"
    >
      <header className="property-story-heading">
        <h2 id="property-short-compare-title">
          {COMPARISON_SURFACE?.title ?? "Compare homes"}
        </h2>
      </header>

      <div className="property-short-compare__homes">
        {homes.map((home) => (
          <article
            key={home.id}
            className={`property-short-compare__home${
              home.isCurrent ? " is-current" : ""
            }`}
          >
            <Link to={propertyHrefWithSearchSpan(home.id, searchSpan)}>
              {showImages && home.heroImage && (
                <span className="property-short-compare__image">
                  <ImageWithFallback
                    src={home.heroImage}
                    alt=""
                    loading="lazy"
                    fetchPriority="low"
                  />
                </span>
              )}
              <span className="property-short-compare__identity">
                {home.isCurrent && <em>Current</em>}
                <strong>{home.title}</strong>
                <span>{home.area}</span>
              </span>
              <dl>
                {dimensions.map((dimension) => (
                  <div key={dimension.key}>
                    <dt>{dimension.label}</dt>
                    <dd>{dimensionValue(home, dimension)}</dd>
                  </div>
                ))}
              </dl>
            </Link>
          </article>
        ))}
      </div>

      <div className="property-short-compare__handoff">
        <Link to={hrefWithSearchSpan(
          compareHref,
          searchSpanReferenceForTarget(searchSpan),
        )}>Open full Compare ↗</Link>
      </div>
    </section>
  );
}
