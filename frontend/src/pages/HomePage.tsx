import { useCallback, useEffect, useState, useRef } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import type { AreaTrackerResponse, DiscoveryResponse, PropertyCard } from "../lib/types.ts";
import { getAreaTracker, getAreas, getDiscovery, getProperties, type PlatformStats } from "../lib/api.ts";
import { getRecentSearches, addRecentSearch, clearRecentSearches } from "../lib/recent-searches.ts";
import { getSavedIds, SAVED_UPDATED_EVENT } from "../lib/sheet-store.ts";
import { SearchExperience as InlineSearchExperience } from "./ResultsPageA.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(0)} L`;
  return price.toLocaleString("en-IN");
}

const ROTATING_WORDS = [
  "transparent homes",
  "honest pricing",
  "trusted societies",
  "clear tradeoffs",
  "real insights",
];

function RotatingText() {
  const [index, setIndex] = useState(0);
  const [fading, setFading] = useState(false);

  useEffect(() => {
    const interval = setInterval(() => {
      setFading(true);
      setTimeout(() => {
        setIndex((i) => (i + 1) % ROTATING_WORDS.length);
        setFading(false);
      }, 400);
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  return (
    <span
      style={{
        display: "inline-block",
        transition: "opacity 0.4s ease, transform 0.4s ease",
        opacity: fading ? 0 : 1,
        transform: fading ? "translateY(8px)" : "translateY(0)",
        color: "var(--color-accent)",
      }}
    >
      {ROTATING_WORDS[index]}
    </span>
  );
}

/* ---------- Popular search suggestions ---------- */
const POPULAR_SEARCHES = [
  "3BHK Whitefield under 2Cr",
  "Family-friendly Sarjapur",
  "Premium 4BHK Koramangala",
  "Near metro Bellandur",
  "Quiet 2BHK HSR Layout",
  "New launch Hebbal",
];

type MarketSnapshot = {
  totalProperties: number;
  totalSocieties: number;
  totalAreas: number;
};

function deriveMarketSnapshot(props: PropertyCard[]): MarketSnapshot {
  const areaMap: Record<string, number> = {};
  for (const p of props) {
    areaMap[p.area] = (areaMap[p.area] ?? 0) + 1;
  }

  return {
    totalProperties: props.length,
    totalSocieties: new Set(props.map((p) => p.society_name)).size,
    totalAreas: Object.keys(areaMap).length,
  };
}

export function HomePage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeSearchQuery = searchParams.get("q") || "";
  const hasActiveSearch = activeSearchQuery.trim().length > 0;
  const hasInlinePane = hasActiveSearch;
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [platformStats, setPlatformStats] = useState<PlatformStats | null>(null);
  const [areaTracker, setAreaTracker] = useState<AreaTrackerResponse | null>(null);
  const [discovery, setDiscovery] = useState<DiscoveryResponse | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState(activeSearchQuery);
  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
  const [sheetCount, setSheetCount] = useState(() => getSavedIds().length);
  const inlineResultsRef = useRef<HTMLElement | null>(null);
  const shouldScrollToResultsRef = useRef(false);

  useEffect(() => {
    if (searchParams.get("view") === "saved") {
      navigate("/results?view=saved", { replace: true });
    }
  }, [navigate, searchParams]);

  useEffect(() => {
    if (hasActiveSearch) return;
    const controller = new AbortController();
    let cancelled = false;
    const timer = window.setTimeout(() => {
      Promise.all([getProperties({ signal: controller.signal }), getAreas({ signal: controller.signal })])
        .then(([props, areas]) => {
          if (cancelled) return;
          setProperties(props);
          setPlatformStats({
            properties: props.length,
            societies: new Set(props.map((p) => p.society_name).filter(Boolean)).size,
            areas: areas.length,
          });
        })
        .catch((error) => {
          if (!cancelled && !(error instanceof DOMException && error.name === "AbortError")) {
            setLoadError(true);
          }
        });
      getAreaTracker({ signal: controller.signal })
        .then((tracker) => {
          if (!cancelled) setAreaTracker(tracker);
        })
        .catch(() => {});
      getDiscovery({ signal: controller.signal })
        .then((home) => {
          if (!cancelled) setDiscovery(home);
        })
        .catch(() => {});
    }, 750);
    return () => {
      cancelled = true;
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [hasActiveSearch]);

  useEffect(() => {
    setQuery(activeSearchQuery);
  }, [activeSearchQuery]);

  useEffect(() => {
    const refreshSheetCount = () => setSheetCount(getSavedIds().length);
    const refreshOnVisible = () => {
      if (!document.hidden) refreshSheetCount();
    };

    refreshSheetCount();
    window.addEventListener("focus", refreshSheetCount);
    window.addEventListener("storage", refreshSheetCount);
    window.addEventListener(SAVED_UPDATED_EVENT, refreshSheetCount);
    document.addEventListener("visibilitychange", refreshOnVisible);

    return () => {
      window.removeEventListener("focus", refreshSheetCount);
      window.removeEventListener("storage", refreshSheetCount);
      window.removeEventListener(SAVED_UPDATED_EVENT, refreshSheetCount);
      document.removeEventListener("visibilitychange", refreshOnVisible);
    };
  }, []);

  useEffect(() => {
    if (!hasInlinePane || !shouldScrollToResultsRef.current) return;
    shouldScrollToResultsRef.current = false;
    window.setTimeout(() => {
      inlineResultsRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 90);
  }, [activeSearchQuery, hasInlinePane]);

  const commitSearch = useCallback((rawQuery: string, options: { scroll?: boolean } = {}) => {
    const q = rawQuery.trim();
    const nextParams = new URLSearchParams();
    setQuery(q);
    if (q) {
      sessionStorage.setItem("oe_search_query", q);
      addRecentSearch(q);
      setRecents(getRecentSearches());
      shouldScrollToResultsRef.current = options.scroll ?? true;
      nextParams.set("q", q);
      setSearchParams(nextParams);
    } else {
      sessionStorage.removeItem("oe_search_query");
      shouldScrollToResultsRef.current = false;
      setSearchParams(nextParams);
    }
  }, [setSearchParams]);

  const handleInlineSearchCommit = useCallback((q: string) => {
    addRecentSearch(q);
    setRecents(getRecentSearches());
  }, []);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    commitSearch(query);
  };

  const derivedSnapshot = !loadError && properties.length > 0 ? deriveMarketSnapshot(properties) : null;
  const snapshot = derivedSnapshot && platformStats
    ? {
        ...derivedSnapshot,
        totalProperties: platformStats.properties,
        totalSocieties: platformStats.societies,
        totalAreas: platformStats.areas,
      }
    : derivedSnapshot;

  return (
    <div>
      {/* Hero */}
      <section
        className={`home-hero ${hasInlinePane ? "home-hero--search-active" : ""}`}
        style={{
          minHeight: hasInlinePane ? "min(72vh, 640px)" : "96vh",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: hasInlinePane
            ? "7rem clamp(1.5rem, 4vw, 4rem) 5rem"
            : "0 clamp(1.5rem, 4vw, 4rem)",
          position: "relative",
          overflow: "hidden",
          transition: "min-height 0.7s var(--ease-out), padding 0.7s var(--ease-out)",
        }}
      >
        <div className="home-hero__wash" aria-hidden="true" />

        <div className="fade-up" style={{ textAlign: "center", maxWidth: "720px" }}>
          <h1 className="home-hero__title">
            Discover{" "}
            <RotatingText />
          </h1>
        </div>

        <p
          className="fade-up fade-up-delay-1"
          style={{
            fontSize: "clamp(1.05rem, 0.95rem + 0.5vw, 1.35rem)",
            color: "#666",
            maxWidth: "520px",
            textAlign: "center",
            margin: "0 0 3rem",
            lineHeight: 1.7,
          }}
        >
          {discovery?.product_promise ?? "Property discovery that explains why, not just what."}
        </p>

        {discovery?.quotes?.length ? (
          <div className="home-proof-quotes fade-up fade-up-delay-1" aria-label="OpenEstates principles">
            {discovery.quotes.slice(0, 3).map((quote) => (
              <span key={quote.text} className={`home-proof-quote home-proof-quote--${quote.tone}`}>
                {quote.text}
              </span>
            ))}
          </div>
        ) : null}

        {/* Search bar */}
        <p className="fade-up fade-up-delay-1 home-hero__kicker">
          Describe what you're looking for
        </p>
        <form
          onSubmit={handleSearch}
          className="search-container home-search-form"
          aria-label="Property search"
          role="search"
        >
          <div className="home-search-input-shell">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#999" strokeWidth="2" strokeLinecap="round">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              className="search-input"
              type="text"
              placeholder="Try: 3BHK Whitefield under 1.5Cr"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Describe the property you are looking for"
            />
          </div>
          <div className="home-search-actions">
            <button
              type="submit"
              className="home-search-submit"
            >
              Search
            </button>
          </div>
        </form>

        {sheetCount > 0 && (
          <Link
            to="/results?view=saved"
            className="home-saved-link fade-up fade-up-delay-2"
            aria-label={`View your ${sheetCount} saved ${sheetCount === 1 ? "home" : "homes"}`}
          >
            {sheetCount} saved {sheetCount === 1 ? "home" : "homes"}
          </Link>
        )}

        {/* Error banner — non-blocking */}
        {loadError && (
          <div className="home-error-banner fade-up fade-up-delay-2">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#92400e" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
            <span>Market data temporarily unavailable. Search still works.</span>
            <button
              onClick={() => window.location.reload()}
              style={{
                background: "none",
                border: "1px solid rgba(146,64,14,0.3)",
                color: "#92400e",
                padding: "0.25rem 0.75rem",
                borderRadius: "6px",
                fontSize: "0.78rem",
                cursor: "pointer",
                fontFamily: "inherit",
                marginLeft: "0.5rem",
                whiteSpace: "nowrap",
              }}
            >
              Retry
            </button>
          </div>
        )}

        {/* Inline stats — social proof */}
        {snapshot && (
          <div
            className="fade-up fade-up-delay-3 home-stats-row"
          >
            <span><strong>{snapshot.totalProperties}</strong> properties</span>
            <span style={{ width: 3, height: 3, borderRadius: "50%", backgroundColor: "#ccc" }} />
            <span><strong>{snapshot.totalSocieties}</strong> societies</span>
            <span style={{ width: 3, height: 3, borderRadius: "50%", backgroundColor: "#ccc" }} />
            <span><strong>{snapshot.totalAreas}</strong> micro-markets</span>
          </div>
        )}

        {/* Popular searches — clickable chips */}
        <div
          className="fade-up fade-up-delay-3"
          style={{
            marginTop: "1.25rem",
            display: "flex",
            gap: "0.5rem",
            flexWrap: "wrap",
            justifyContent: "center",
            maxWidth: "600px",
          }}
        >
          {POPULAR_SEARCHES.slice(0, 4).map((s) => (
            <button
              key={s}
              type="button"
              className="home-popular-chip"
              onClick={() => {
                commitSearch(s);
              }}
            >
              {s}
            </button>
          ))}
        </div>

        {/* Recent searches */}
        {recents.length > 0 && (
          <div className="fade-up fade-up-delay-3 recent-searches">
            <span className="recent-searches-label">Recent</span>
            {recents.map((s) => (
              <button
                key={s}
                className="empty-state-chip"
                onClick={() => {
                  commitSearch(s);
                }}
              >
                {s}
              </button>
            ))}
            <button
              className="recent-clear-btn"
              onClick={() => { clearRecentSearches(); setRecents([]); }}
            >
              clear
            </button>
          </div>
        )}

      </section>

      {hasInlinePane && (
        <section ref={inlineResultsRef} className="home-inline-results-anchor" aria-label="Search results">
          <InlineSearchExperience
            variant="embedded"
            onSearchCommit={handleInlineSearchCommit}
          />
        </section>
      )}

      {!hasInlinePane && discovery?.shelves?.length ? (
        <DiscoveryShelvesSection discovery={discovery} onSearch={commitSearch} />
      ) : null}

      {!hasInlinePane && snapshot && properties.length > 0 && (
        <MicroMarketsSection properties={properties} areaTracker={areaTracker} onSearch={commitSearch} />
      )}
    </div>
  );
}

/* ---------- Sub-components ---------- */

/* ---------- Micro-Market Intelligence ---------- */

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
      {/* Header */}
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

      {/* Signal chips */}
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

      {/* Top builder */}
      {m.topBuilder && (
        <p style={{ margin: "0.6rem 0 0", fontSize: "0.72rem", color: "#aaa" }}>
          Top builder: <span style={{ color: "#777", fontWeight: 500 }}>{m.topBuilder}</span>
        </p>
      )}
    </button>
  );
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

function DiscoveryShelvesSection({
  discovery,
  onSearch,
}: {
  discovery: DiscoveryResponse;
  onSearch: (query: string) => void;
}) {
  const shelves = discovery.shelves.filter((shelf) => shelf.cards.length > 0).slice(0, 5);
  if (shelves.length === 0) return null;

  return (
    <section className="home-discovery-section" aria-label="Discovery shelves">
      <div className="home-discovery-inner">
        <div className="home-discovery-heading-row">
          <div>
            <span className="home-discovery-kicker">Discovery shelves</span>
            <h2 className="home-discovery-heading">Curated by intent</h2>
          </div>
          <button
            type="button"
            className="home-discovery-all"
            onClick={() => onSearch("transparent homes with proof")}
          >
            Explore by intent
          </button>
        </div>

        <div className="home-discovery-grid">
          {shelves.map((shelf) => (
            <article key={shelf.id} className="home-discovery-shelf">
              <div className="home-discovery-shelf__head">
                <span>{shelf.receipt_copy}</span>
                <h3>{shelf.title}</h3>
                <p>{shelf.quote}</p>
              </div>
              <p className="home-discovery-shelf__desc">{shelf.description}</p>
              <div className="home-discovery-cards">
                {shelf.cards.slice(0, 3).map(({ property, reason }) => (
                  <Link
                    key={`${shelf.id}-${property.id}`}
                    to={`/property/${property.id}`}
                    className="home-discovery-card"
                  >
                    <strong>{property.society_name || property.title}</strong>
                    <span>{property.area} · {property.bhk} BHK · {formatPrice(property.price)}</span>
                    <em>{reason}</em>
                  </Link>
                ))}
              </div>
              <button
                type="button"
                className="home-discovery-search"
                onClick={() => onSearch(shelf.search_query)}
              >
                Search this shelf
              </button>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

function MicroMarketsSection({
  properties,
  areaTracker,
  onSearch,
}: {
  properties: PropertyCard[];
  areaTracker: AreaTrackerResponse | null;
  onSearch: (query: string) => void;
}) {
  const markets = areaTracker
    ? deriveMicroMarketsFromTracker(areaTracker, properties)
    : deriveMicroMarkets(properties);
  if (markets.length < 2) return null;

  const maxAvg = Math.max(1, ...markets.filter((m) => m.hasAvgPriceSqft).map((m) => m.avgPriceSqft));

  return (
    <section id="area-tracker" className="home-micro-section" style={{ padding: "2.5rem clamp(1.5rem, 4vw, 4rem) 2rem" }}>
      <div style={{ maxWidth: "960px", margin: "0 auto" }}>
        <div style={{ marginBottom: "1.5rem" }}>
          <h2 className="home-micro-heading">Area Tracker</h2>
          <p style={{ margin: 0, fontSize: "0.85rem", color: "#888" }}>
            {markets.length} Bengaluru areas · {properties.length} listings · price, access, and trust signals
          </p>
        </div>
        <div className="home-micro-grid">
          {markets.map((m) => (
            <MicroMarketCard key={m.area} m={m} maxAvg={maxAvg} onSearch={onSearch} />
          ))}
        </div>
      </div>
    </section>
  );
}
