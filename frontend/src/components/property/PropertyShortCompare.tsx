import { Link } from "react-router-dom";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
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

export function PropertyShortCompare({ homes, compareHref }: Props) {
  if (homes.length !== 3 || !compareHref) return null;
  const showMedia = homes.every((home) => Boolean(home.heroImage));

  return (
    <section
      className="property-fact-deck property-short-compare"
      aria-labelledby="property-short-compare-title"
    >
      <header className="property-fact-deck__intro">
        <span>Short comparison</span>
        <h2 id="property-short-compare-title">Three homes. Facts first.</h2>
      </header>

      <div className="property-short-compare__grid">
        {homes.map((home) => {
          const price = formatPrice(home.price);
          return (
            <article key={home.id} className="property-short-compare__card">
              <Link
                className="property-short-compare__home"
                to={`/property/${encodeURIComponent(home.id)}`}
              >
                {showMedia && (
                  <div className="property-short-compare__media">
                    <ImageWithFallback
                      src={home.heroImage ?? ""}
                      alt=""
                      loading="lazy"
                      fetchPriority="low"
                    />
                  </div>
                )}
                <div className="property-short-compare__copy">
                  <div className="property-short-compare__eyebrow">
                    <span>{home.isCurrent ? "This home" : "Compare"}</span>
                    <span>{home.area}</span>
                  </div>
                  <h3>{home.title}</h3>
                  <dl>
                    {price && (
                      <div>
                        <dt>Price</dt>
                        <dd>{price}</dd>
                      </div>
                    )}
                    {home.bhk && (
                      <div>
                        <dt>Home</dt>
                        <dd>{home.bhk} BHK</dd>
                      </div>
                    )}
                    {home.status && (
                      <div>
                        <dt>Status</dt>
                        <dd>{home.status}</dd>
                      </div>
                    )}
                  </dl>
                </div>
              </Link>
            </article>
          );
        })}
      </div>

      <div className="property-short-compare__handoff">
        <span>Continue with these three homes</span>
        <Link to={compareHref}>Open full Compare ↗</Link>
      </div>
    </section>
  );
}
