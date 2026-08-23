import { Link } from "react-router-dom";
import type { StoryComparison } from "../../lib/propertyStory.ts";
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
  if (homes.length < 2 || homes.length > 4 || !compareHref) return null;
  const dimensions = COMPARISON_DIMENSIONS.filter((dimension) =>
    homes.every((home) => Boolean(dimension.value(home))));
  if (dimensions.length === 0) return null;

  return (
    <section
      id="short-compare"
      className="property-fact-deck property-short-compare"
      aria-labelledby="property-short-compare-title"
    >
      <header className="property-story-heading">
        <span>Compare</span>
        <h2 id="property-short-compare-title">Saved homes. Same facts.</h2>
      </header>

      <div className="property-short-compare__table-wrap">
        <table className="property-short-compare__table">
          <thead>
            <tr>
              <th scope="col">Fact</th>
              {homes.map((home, index) => (
                <th
                  key={home.id}
                  scope="col"
                  className={home.isCurrent ? "is-current" : ""}
                >
                  <Link to={`/property/${encodeURIComponent(home.id)}`}>
                    <i aria-hidden="true">{String(index + 1).padStart(2, "0")}</i>
                    <strong>{home.title}</strong>
                    <span>{home.area}</span>
                    {home.isCurrent && <em>Current home</em>}
                  </Link>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {dimensions.map((dimension) => (
              <tr key={dimension.key}>
                <th scope="row">{dimension.label}</th>
                {homes.map((home) => (
                  <td
                    key={home.id}
                    className={home.isCurrent ? "is-current" : ""}
                  >
                    {dimension.value(home)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="property-short-compare__handoff">
        <Link to={compareHref}>Open full Compare ↗</Link>
      </div>
    </section>
  );
}
