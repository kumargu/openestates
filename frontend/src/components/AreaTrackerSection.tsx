import { Link } from "react-router-dom";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(0)} L`;
  return price.toLocaleString("en-IN");
}

const AREA_VIBES: Record<string, string> = {
  Whitefield: "Tech hub with new metro access",
  "Sarjapur Road": "Fast-growing IT corridor",
  Bellandur: "Premium zone, lake concerns",
  "HSR Layout": "Walkable startup neighbourhood",
  Koramangala: "Bengaluru's most vibrant area",
  Hebbal: "North Bengaluru premium corridor",
  Marathahalli: "Affordable ORR hub",
  Thanisandra: "Emerging north with IT access",
  "Electronic City": "Affordable tech zone",
  Hoodi: "Fast-developing metro hub",
  Panathur: "Quiet residential pocket",
  Varthur: "Emerging eastern suburb",
};

type MicroMarket = {
  area: string;
  vibe: string;
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

function deriveMicroMarkets(properties: PropertyCard[]): MicroMarket[] {
  const byArea: Record<string, PropertyCard[]> = {};
  for (const p of properties) {
    (byArea[p.area] ??= []).push(p);
  }

  return Object.entries(byArea)
    .filter(([, ps]) => ps.length >= 2)
    .map(([area, ps]) => {
      const prices = ps.map((p) => p.price_per_sqft).filter((price) => price > 0);
      const projectPrices = ps.map((p) => p.price).filter((price) => price > 0);
      const bhkSet = new Set(ps.map((p) => p.bhk));
      const builderCount: Record<string, number> = {};
      for (const p of ps) {
        builderCount[p.builder_name] = (builderCount[p.builder_name] ?? 0) + 1;
      }
      const topBuilder = Object.entries(builderCount).sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
      const societies = new Set(ps.map((p) => p.society_name));

      return {
        area,
        vibe: AREA_VIBES[area] ?? "",
        avgPriceSqft: prices.length > 0 ? Math.round(prices.reduce((a, b) => a + b, 0) / prices.length) : 0,
        hasAvgPriceSqft: prices.length > 0,
        priceMin: projectPrices.length > 0 ? Math.min(...projectPrices) : 0,
        priceMax: projectPrices.length > 0 ? Math.max(...projectPrices) : 0,
        count: ps.length,
        bhks: Array.from(bhkSet).sort(),
        readyToMove: ps.filter((p) => p.possession_status === "ready").length,
        nearMetro: ps.filter((p) => p.metro_distance_mins <= 15).length,
        topBuilder,
        societies: societies.size,
      };
    })
    .sort((a, b) => b.count - a.count);
}

function deriveMicroMarketsFromTracker(
  tracker: AreaTrackerResponse,
  _properties: PropertyCard[],
): MicroMarket[] {
  return tracker.markets.map((market) => ({
    area: market.name,
    vibe: AREA_VIBES[market.name] ?? "",
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

function chipStyle(bg: string, color: string): React.CSSProperties {
  return {
    fontSize: "0.68rem",
    padding: "0.15rem 0.5rem",
    borderRadius: "999px",
    backgroundColor: bg,
    color,
    fontWeight: 500,
    whiteSpace: "nowrap",
  };
}

function MicroMarketCard({
  m,
  maxAvg,
  onSearch,
}: {
  m: MicroMarket;
  maxAvg: number;
  onSearch: (query: string) => void;
}) {
  const hasPriceRange = m.priceMin > 0 && m.priceMax > 0;
  const pct = m.hasAvgPriceSqft ? (m.avgPriceSqft / maxAvg) * 100 : 0;
  const barColor =
    pct > 85 ? "var(--color-accent)" : pct > 60 ? "var(--color-cool-deep)" : "var(--color-cool)";

  return (
    <button
      type="button"
      className="home-micro-card"
      onClick={() => onSearch(m.area)}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "0.2rem" }}>
        <span style={{ fontSize: "1.05rem", fontWeight: 600, color: "#1a1a1a", letterSpacing: "-0.01em" }}>
          {m.area}
        </span>
        <span style={{ fontSize: "0.68rem", color: "#aaa", whiteSpace: "nowrap", marginLeft: "0.5rem" }}>
          {m.count} listings
        </span>
      </div>
      {m.vibe && (
        <p style={{ margin: "0 0 0.85rem", fontSize: "0.78rem", color: "#999", lineHeight: 1.3 }}>
          {m.vibe}
        </p>
      )}

      {(m.hasAvgPriceSqft || hasPriceRange) && (
        <div style={{ marginBottom: "0.85rem" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.3rem" }}>
            {m.hasAvgPriceSqft && (
              <span className="home-micro-price">
                {m.avgPriceSqft.toLocaleString("en-IN")}
                <span style={{ fontSize: "0.7rem", color: "#999", fontWeight: 400, marginLeft: "0.2rem" }}>/sqft</span>
              </span>
            )}
            {hasPriceRange && (
              <span style={{ fontSize: "0.72rem", color: "#aaa", marginLeft: "auto" }}>
                {formatPrice(m.priceMin)} – {formatPrice(m.priceMax)}
              </span>
            )}
          </div>
          {m.hasAvgPriceSqft && (
            <div style={{ height: "4px", backgroundColor: "rgba(0,0,0,0.04)", borderRadius: "2px", overflow: "hidden" }}>
              <div
                style={{
                  height: "100%",
                  width: `${pct}%`,
                  borderRadius: "2px",
                  backgroundColor: barColor,
                  transition: "width 0.6s cubic-bezier(0.16, 1, 0.3, 1)",
                }}
              />
            </div>
          )}
        </div>
      )}

      <div style={{ display: "flex", gap: "0.3rem", flexWrap: "wrap" }}>
        {m.bhks.length > 0 && (
          <span style={chipStyle("#f0f0f0", "#555")}>
            {m.bhks.join(", ")} BHK
          </span>
        )}
        {m.societies > 1 && (
          <span style={chipStyle("#f0f0f0", "#555")}>
            {m.societies} societies
          </span>
        )}
        {m.readyToMove > 0 && (
          <span style={chipStyle("var(--color-positive-bg)", "var(--color-positive)")}>
            {m.readyToMove} ready
          </span>
        )}
        {m.nearMetro > 0 && (
          <span className="home-chip-cool">
            {m.nearMetro} near metro
          </span>
        )}
      </div>

      {m.topBuilder && (
        <p style={{ margin: "0.6rem 0 0", fontSize: "0.72rem", color: "#aaa" }}>
          Top builder: <span style={{ color: "#777", fontWeight: 500 }}>{m.topBuilder}</span>
        </p>
      )}
    </button>
  );
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
  heading = "Area Tracker",
  subheading,
}: AreaTrackerSectionProps) {
  const allMarkets = areaTracker
    ? deriveMicroMarketsFromTracker(areaTracker, properties)
    : deriveMicroMarkets(properties);
  if (allMarkets.length < 2) return null;

  const markets = maxMarkets ? allMarkets.slice(0, maxMarkets) : allMarkets;
  const maxAvg = Math.max(1, ...markets.filter((m) => m.hasAvgPriceSqft).map((m) => m.avgPriceSqft));
  const defaultSubheading = `${allMarkets.length} Bengaluru areas · ${properties.length} listings`;

  return (
    <section id={id} className="home-micro-section" style={{ padding: "2.5rem clamp(1.5rem, 4vw, 4rem) 2rem" }}>
      <div style={{ maxWidth: "960px", margin: "0 auto" }}>
        <div style={{ marginBottom: "1.5rem" }}>
          <h2 className="home-micro-heading">{heading}</h2>
          <p style={{ margin: 0, fontSize: "0.85rem", color: "#888" }}>
            {subheading ?? defaultSubheading}
          </p>
        </div>
        <div className="home-micro-grid">
          {markets.map((m) => (
            <MicroMarketCard key={m.area} m={m} maxAvg={maxAvg} onSearch={onSearch} />
          ))}
        </div>
        {footerLink && (
          <div style={{ marginTop: "1.5rem", textAlign: "center" }}>
            <Link to={footerLink.to} className="home-discovery-all">
              {footerLink.label}
            </Link>
          </div>
        )}
      </div>
    </section>
  );
}
