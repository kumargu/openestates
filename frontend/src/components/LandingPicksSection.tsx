import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { landingPickRails } from "../lib/landing-picks.ts";
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
  const rails = landingPickRails(properties, areaTracker, Math.min(maxPicks, 7));

  if (rails.length === 0) return null;

  return (
    <section className="home-picks-section" aria-label="Home picks">
      <div className="home-picks-section__inner">
        {rails.map((rail) => (
          <section key={rail.id} className="home-picks-rail" aria-label={rail.title}>
            <div className="home-picks-rail__head">
              <h3>{rail.title}</h3>
            </div>
            <div className="home-picks-rail__scroller">
              {rail.picks.map(({ area, property }) => (
                <div key={`${rail.id}-${area}-${property.id}`} className="home-picks-section__item">
                  <LivingEvidenceTile property={property} variant="browse" />
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </section>
  );
}
