import { Link } from "react-router-dom";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";
import {
  AreaPriceBands,
  type AreaMarketContext,
} from "./AreaPriceBands.tsx";

function deriveMarketContexts(properties: PropertyCard[]): AreaMarketContext[] {
  const byArea: Record<string, PropertyCard[]> = {};
  for (const property of properties) {
    (byArea[property.area] ??= []).push(property);
  }

  return Object.entries(byArea)
    .filter(([, areaProperties]) => areaProperties.length >= 2)
    .map(([area, areaProperties]) => {
      const homePrices = areaProperties
        .map((property) => property.price)
        .filter((price) => price > 0);
      return {
        area,
        homePriceMin: homePrices.length > 0 ? Math.min(...homePrices) : 0,
        homePriceMax: homePrices.length > 0 ? Math.max(...homePrices) : 0,
        bhks: [...new Set(areaProperties.map((property) => property.bhk))].sort(),
        societies: new Set(areaProperties.map((property) => property.society_name)).size,
      };
    });
}

function deriveMarketContextsFromTracker(
  tracker: AreaTrackerResponse,
): AreaMarketContext[] {
  return tracker.markets.map((market) => ({
    area: market.name,
    homePriceMin: market.price_min,
    homePriceMax: market.price_max,
    bhks: market.bhks,
    societies: market.societies,
  }));
}

export type AreaTrackerSectionProps = {
  properties: PropertyCard[];
  areaTracker: AreaTrackerResponse | null;
  onSearch: (query: string) => void;
  maxMarkets?: number;
  footerLink?: { to: string; label: string };
  id?: string;
  heading?: string;
  subheading?: string;
};

export function AreaTrackerSection({
  properties,
  areaTracker,
  onSearch,
  maxMarkets,
  footerLink,
  id = "area-tracker",
  heading = "Market map",
  subheading = "Where asks sit across Bengaluru — tap an area to search it.",
}: AreaTrackerSectionProps) {
  const allMarkets = areaTracker
    ? deriveMarketContextsFromTracker(areaTracker)
    : deriveMarketContexts(properties);
  const markets = maxMarkets ? allMarkets.slice(0, maxMarkets) : allMarkets;
  if (markets.length < 2) return null;

  return (
    <section id={id} className="home-micro-section area-tracker-section">
      <div className="area-tracker-section__inner">
        <AreaPriceBands
          properties={properties}
          preferredAreas={markets.map((m) => m.area)}
          marketContexts={markets}
          onSelectArea={onSearch}
          heading={heading}
          subheading={subheading}
        />

        {footerLink && (
          <div className="area-tracker-section__footer">
            <Link to={footerLink.to} className="home-discovery-all">
              {footerLink.label}
            </Link>
          </div>
        )}
      </div>
    </section>
  );
}
