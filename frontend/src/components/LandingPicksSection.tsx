import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { topGoogleRatedPerArea, areaNamesForLandingPicks } from "../lib/landing-picks.ts";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";

export type LandingPicksSectionProps = {
  properties: PropertyCard[];
  areaTracker: AreaTrackerResponse | null;
  maxPicks?: number;
};

export function LandingPicksSection({
  properties,
  areaTracker,
  maxPicks = 12,
}: LandingPicksSectionProps) {
  const areaNames = areaNamesForLandingPicks(areaTracker, properties);
  const picks = topGoogleRatedPerArea(properties, areaNames).slice(0, maxPicks);

  if (picks.length === 0) return null;

  return (
    <section className="home-picks-section" aria-label="Top-rated homes by area">
      <div className="home-picks-section__inner">
        <div className="home-picks-section__head">
          <span className="home-picks-section__kicker">Area Tracker picks</span>
          <h2 className="home-picks-section__title">Top-rated in each area</h2>
        </div>
        <div className="results-grid home-picks-section__grid">
          {picks.map(({ area, property }) => (
            <div key={`${area}-${property.id}`} className="home-picks-section__item">
              <span className="home-picks-section__area">{area}</span>
              <LivingEvidenceTile property={property} variant="browse" />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
