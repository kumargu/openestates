import { Link } from "react-router-dom";
import type { StoryComparison } from "../../lib/propertyStory.ts";
import {
  PropertyEvidenceCard,
  type PropertyEvidenceFact,
} from "./PropertyEvidenceCard.tsx";
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

type ComparisonDimension = {
  key: string;
  label: string;
  value: (home: StoryComparison) => string | undefined;
};

const COMPARISON_DIMENSIONS: ComparisonDimension[] = [
  { key: "price", label: "Price", value: (home) => formatPrice(home.price) },
  {
    key: "configuration",
    label: "Home",
    value: (home) => home.bhk ? `${home.bhk} BHK` : undefined,
  },
  { key: "size", label: "Area", value: (home) => home.sizeLabel },
  { key: "status", label: "Status", value: (home) => home.status },
];

export function PropertyShortCompare({ homes, compareHref }: Props) {
  if (homes.length !== 3 || !compareHref) return null;
  const showMedia = homes.every((home) => Boolean(home.heroImage));
  const dimensions = COMPARISON_DIMENSIONS.filter((dimension) =>
    homes.every((home) => Boolean(dimension.value(home))));

  return (
    <section
      id="short-compare"
      className="property-fact-deck property-short-compare"
      aria-labelledby="property-short-compare-title"
    >
      <header className="property-story-heading">
        <span>Compare</span>
        <h2 id="property-short-compare-title">Three homes. Same facts.</h2>
      </header>

      <div className="property-evidence-grid property-evidence-grid--compare">
        {homes.map((home) => {
          const facts: PropertyEvidenceFact[] = dimensions.flatMap(
            (dimension) => {
              const value = dimension.value(home);
              return value
                ? [{
                    key: dimension.key,
                    label: dimension.label,
                    value,
                  }]
                : [];
            },
          );
          return (
            <PropertyEvidenceCard
              key={home.id}
              to={`/property/${encodeURIComponent(home.id)}`}
              eyebrow={home.area}
              title={home.title}
              facts={facts}
              footer="View home"
              imageUrl={showMedia ? home.heroImage : undefined}
              imageAlt=""
              current={home.isCurrent}
            />
          );
        })}
      </div>

      <div className="property-short-compare__handoff">
        <Link to={compareHref}>Open full Compare ↗</Link>
      </div>
    </section>
  );
}
