import { Link } from "react-router-dom";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(0)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

type MicroMarket = {
  area: string;
  avgPriceSqft: number;
  hasAvgPriceSqft: boolean;
  priceMin: number;
  priceMax: number;
  count: number;
  bhks: number[];
  readyToMove: number;
  nearMetro: number;
  topBuilder: string;
  societies: number;
};

function toMicroMarket(area: string, areaProperties: PropertyCard[]): MicroMarket {
  const prices = areaProperties
    .map((property) => property.price_per_sqft)
    .filter((price) => price > 0);
  const projectPrices = areaProperties
    .map((property) => property.price)
    .filter((price) => price > 0);
  const builderCount: Record<string, number> = {};
  for (const property of areaProperties) {
    builderCount[property.builder_name] = (builderCount[property.builder_name] ?? 0) + 1;
  }
  const topBuilder =
    Object.entries(builderCount).sort((left, right) => right[1] - left[1])[0]?.[0] ?? "";

  return {
    area,
    avgPriceSqft: prices.length > 0
      ? Math.round(prices.reduce((sum, price) => sum + price, 0) / prices.length)
      : 0,
    hasAvgPriceSqft: prices.length > 0,
    priceMin: projectPrices.length > 0 ? Math.min(...projectPrices) : 0,
    priceMax: projectPrices.length > 0 ? Math.max(...projectPrices) : 0,
    count: areaProperties.length,
    bhks: [...new Set(areaProperties
      .map((property) => property.bhk)
      .filter((bhk) => bhk > 0))].sort((a, b) => a - b),
    readyToMove: areaProperties.filter((property) =>
      property.possession_status === "ready"
      || property.project_status === "ready_to_move"
    ).length,
    nearMetro: areaProperties.filter((property) => property.metro_distance_mins > 0 && property.metro_distance_mins <= 15).length,
    topBuilder,
    societies: new Set(areaProperties.map((property) => property.society_name)).size,
  };
}

function deriveMicroMarkets(properties: PropertyCard[]): MicroMarket[] {
  const byArea: Record<string, PropertyCard[]> = {};
  for (const property of properties) {
    (byArea[property.area] ??= []).push(property);
  }

  return Object.entries(byArea)
    .filter(([, areaProperties]) => areaProperties.length >= 2)
    .map(([area, areaProperties]) => toMicroMarket(area, areaProperties))
    .sort((left, right) => right.count - left.count);
}

function derivePreferredMicroMarkets(
  properties: PropertyCard[],
  preferredAreas: string[],
): MicroMarket[] {
  const byArea: Record<string, PropertyCard[]> = {};
  for (const property of properties) {
    (byArea[property.area] ??= []).push(property);
  }

  return preferredAreas
    .map((area) => {
      const areaProperties = byArea[area];
      if (!areaProperties || areaProperties.length === 0) return null;
      return toMicroMarket(area, areaProperties);
    })
    .filter((market): market is MicroMarket => market !== null);
}

function deriveMicroMarketsFromTracker(tracker: AreaTrackerResponse): MicroMarket[] {
  return tracker.markets.map((market) => ({
    area: market.name,
    avgPriceSqft: market.avg_price_per_sqft,
    hasAvgPriceSqft: market.avg_price_per_sqft > 0,
    priceMin: market.price_min,
    priceMax: market.price_max,
    count: market.listing_count,
    bhks: market.bhks,
    readyToMove: market.ready_to_move,
    nearMetro: market.near_metro,
    topBuilder: market.top_builder,
    societies: market.societies,
  }));
}

function MicroMarketCard({
  market,
  maxAvg,
  onSearch,
  highlighted,
}: {
  market: MicroMarket;
  maxAvg: number;
  onSearch: (query: string) => void;
  highlighted?: boolean;
}) {
  const hasPriceRange = market.priceMin > 0 && market.priceMax > 0;
  const pct = market.hasAvgPriceSqft ? (market.avgPriceSqft / maxAvg) * 100 : 0;
  const barTone =
    pct > 85 ? "home-micro-card__bar--hot"
    : pct > 60 ? "home-micro-card__bar--mid"
    : "home-micro-card__bar--cool";

  return (
    <button
      type="button"
      className={`home-micro-card${highlighted ? " is-current" : ""}`}
      onClick={() => onSearch(market.area)}
    >
      <div className="home-micro-card__top">
        <span className="home-micro-card__area">{market.area}</span>
        <span className="home-micro-card__count">{market.count}</span>
      </div>

      {(market.hasAvgPriceSqft || hasPriceRange) && (
        <div className="home-micro-card__price-block">
          <div className="home-micro-card__price-row">
            {market.hasAvgPriceSqft && (
              <span className="home-micro-price">
                {market.avgPriceSqft.toLocaleString("en-IN")}
                <span className="home-micro-price__unit">/sqft</span>
              </span>
            )}
            {hasPriceRange && (
              <span className="home-micro-card__range">
                {formatPrice(market.priceMin)} – {formatPrice(market.priceMax)}
              </span>
            )}
          </div>
          {market.hasAvgPriceSqft && (
            <div className="home-micro-card__track">
              <div
                className={`home-micro-card__bar ${barTone}`}
                style={{ width: `${Math.max(8, Math.min(100, pct))}%` }}
              />
            </div>
          )}
        </div>
      )}

      <div className="home-micro-card__chips">
        {market.bhks.length > 0 && (
          <span className="home-micro-chip">{market.bhks.join(", ")} BHK</span>
        )}
        {market.societies > 1 && (
          <span className="home-micro-chip">{market.societies} societies</span>
        )}
        {market.readyToMove > 0 && (
          <span className="home-micro-chip home-micro-chip--ready">{market.readyToMove} ready</span>
        )}
        {market.nearMetro > 0 && (
          <span className="home-chip-cool">{market.nearMetro} near metro</span>
        )}
      </div>

      {market.topBuilder && (
        <p className="home-micro-card__builder">{market.topBuilder}</p>
      )}
    </button>
  );
}

export type AreaTrackerSectionProps = {
  properties: PropertyCard[];
  areaTracker: AreaTrackerResponse | null;
  onSearch: (query: string) => void;
  maxMarkets?: number;
  preferredAreas?: string[];
  highlightArea?: string;
  footerLink?: { to: string; label: string };
  id?: string;
  heading?: string;
  className?: string;
};

export function AreaTrackerSection({
  properties,
  areaTracker,
  onSearch,
  maxMarkets,
  preferredAreas,
  highlightArea,
  footerLink,
  id = "area-tracker",
  heading = "Area Tracker",
  className = "",
}: AreaTrackerSectionProps) {
  const allMarkets = preferredAreas && preferredAreas.length > 0
    ? derivePreferredMicroMarkets(properties, preferredAreas)
    : areaTracker
      ? deriveMicroMarketsFromTracker(areaTracker)
      : deriveMicroMarkets(properties);
  if (allMarkets.length < 1) return null;

  const markets = maxMarkets ? allMarkets.slice(0, maxMarkets) : allMarkets;
  const maxAvg = Math.max(
    1,
    ...markets.filter((market) => market.hasAvgPriceSqft).map((market) => market.avgPriceSqft),
  );
  const sectionClass = ["home-micro-section", "area-tracker-section", className]
    .filter(Boolean)
    .join(" ");

  return (
    <section id={id} className={sectionClass} aria-label={heading}>
      <div className="area-tracker-section__inner">
        <div className="area-tracker-section__head">
          <h2 className="home-micro-heading">{heading}</h2>
        </div>
        <div className="home-micro-grid">
          {markets.map((market) => (
            <MicroMarketCard
              key={market.area}
              market={market}
              maxAvg={maxAvg}
              onSearch={onSearch}
              highlighted={Boolean(highlightArea && market.area === highlightArea)}
            />
          ))}
        </div>
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
