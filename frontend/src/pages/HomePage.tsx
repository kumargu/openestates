import { lazy, Suspense, useCallback, useEffect, useState, useRef } from "react";
import { Link, useSearchParams } from "react-router-dom";
import type { PropertyCard } from "../lib/types.ts";
import { getProperties, getStats, type PlatformStats } from "../lib/api.ts";
import { getRecentSearches, addRecentSearch, clearRecentSearches } from "../lib/recent-searches.ts";
import { getSheetItems, SHEET_UPDATED_EVENT } from "../lib/sheet-store.ts";

const InlineSearchExperience = lazy(() =>
  import("./ResultsPageA.tsx").then((m) => ({ default: m.SearchExperience }))
);

function useOnScreen(ref: React.RefObject<HTMLElement | null>) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      ([entry]) => { if (entry.isIntersecting) { setVisible(true); obs.disconnect(); } },
      { threshold: 0.15 }
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [ref]);
  return visible;
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

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(0)} L`;
  return price.toLocaleString("en-IN");
}

/* ---------- Derived market stats ---------- */
type TrendingHighlight = {
  label: string;
  value: string;
  searchQuery: string;
};

type MarketSnapshot = {
  totalProperties: number;
  totalSocieties: number;
  totalAreas: number;
  priceMin: number;
  priceMax: number;
  topBuilders: { name: string; count: number }[];
  bhkBreakdown: Record<number, number>;
  areaPropertyCounts: Record<string, number>;
  trending: TrendingHighlight[];
};

function deriveMarketSnapshot(props: PropertyCard[]): MarketSnapshot {
  const prices = props.map((p) => p.price);
  const builderMap = new Map<string, number>();
  const bhkMap: Record<number, number> = {};
  const areaMap: Record<string, number> = {};

  for (const p of props) {
    builderMap.set(p.builder_name, (builderMap.get(p.builder_name) ?? 0) + 1);
    bhkMap[p.bhk] = (bhkMap[p.bhk] ?? 0) + 1;
    areaMap[p.area] = (areaMap[p.area] ?? 0) + 1;
  }

  const topBuilders = [...builderMap.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5)
    .map(([name, count]) => ({ name, count }));

  // Derive trending highlights from real data
  const trending: TrendingHighlight[] = [];

  // Most listings area
  const topArea = Object.entries(areaMap).sort(([, a], [, b]) => b - a)[0];
  if (topArea) {
    trending.push({
      label: "Most active",
      value: `${topArea[0]} — ${topArea[1]} listings`,
      searchQuery: topArea[0],
    });
  }

  // Best value area (lowest avg price/sqft)
  const areaPrices: Record<string, number[]> = {};
  for (const p of props) {
    (areaPrices[p.area] ??= []).push(p.price_per_sqft);
  }
  const areaAvgs = Object.entries(areaPrices)
    .map(([area, ps]) => ({ area, avg: ps.reduce((a, b) => a + b, 0) / ps.length }))
    .filter((a) => (areaPrices[a.area]?.length ?? 0) >= 3)
    .sort((a, b) => a.avg - b.avg);
  if (areaAvgs.length > 0) {
    trending.push({
      label: "Best value",
      value: `${areaAvgs[0].area} — ${Math.round(areaAvgs[0].avg).toLocaleString("en-IN")}/sqft avg`,
      searchQuery: areaAvgs[0].area,
    });
  }

  // Near metro count
  const metroClose = props.filter((p) => p.metro_distance_mins <= 10).length;
  if (metroClose > 0) {
    trending.push({
      label: "Near metro",
      value: `${metroClose} properties within 10 min`,
      searchQuery: "near metro",
    });
  }

  // Ready to move count
  const readyToMove = props.filter((p) => p.possession_status === "Ready to Move").length;
  if (readyToMove > 0) {
    trending.push({
      label: "Ready to move",
      value: `${readyToMove} available now`,
      searchQuery: "ready to move",
    });
  }

  // Premium segment
  const premium = props.filter((p) => p.price >= 20_000_000).length;
  if (premium > 0) {
    trending.push({
      label: "Premium",
      value: `${premium} listings above 2 Cr`,
      searchQuery: "premium 4BHK",
    });
  }

  return {
    totalProperties: props.length,
    totalSocieties: new Set(props.map((p) => p.society_name)).size,
    totalAreas: Object.keys(areaMap).length,
    priceMin: Math.min(...prices),
    priceMax: Math.max(...prices),
    topBuilders,
    bhkBreakdown: bhkMap,
    areaPropertyCounts: areaMap,
    trending,
  };
}

/* ---------- Featured property (real data) ---------- */
function pickFeatured(props: PropertyCard[]): PropertyCard | null {
  // Pick a well-priced 3BHK with an image as the hero preview
  const candidates = props.filter(
    (p) => p.bhk === 3 && p.hero_image && p.transparency_tags.length > 0
  );
  if (candidates.length === 0) return props.find((p) => p.hero_image) ?? props[0] ?? null;
  // Pick a stable one (middle-ish price)
  candidates.sort((a, b) => a.price - b.price);
  return candidates[Math.floor(candidates.length / 2)];
}

export function HomePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeSearchQuery = searchParams.get("q") || "";
  const hasActiveSearch = activeSearchQuery.trim().length > 0;
  const activeView = searchParams.get("view") === "sheet" ? "sheet" : "cards";
  const hasInlinePane = hasActiveSearch || activeView === "sheet";
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [platformStats, setPlatformStats] = useState<PlatformStats | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState(activeSearchQuery);
  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
  const [sheetCount, setSheetCount] = useState(() => getSheetItems().length);
  const pulseRef = useRef<HTMLElement | null>(null);
  const inlineResultsRef = useRef<HTMLElement | null>(null);
  const shouldScrollToResultsRef = useRef(false);
  const pulseVisible = useOnScreen(pulseRef);

  useEffect(() => {
    getProperties()
      .then(setProperties)
      .catch(() => setLoadError(true));
    getStats()
      .then(setPlatformStats)
      .catch(() => {});
  }, []);

  useEffect(() => {
    setQuery(activeSearchQuery);
  }, [activeSearchQuery]);

  useEffect(() => {
    const refreshSheetCount = () => setSheetCount(getSheetItems().length);
    const refreshOnVisible = () => {
      if (!document.hidden) refreshSheetCount();
    };

    refreshSheetCount();
    window.addEventListener("focus", refreshSheetCount);
    window.addEventListener("storage", refreshSheetCount);
    window.addEventListener(SHEET_UPDATED_EVENT, refreshSheetCount);
    document.addEventListener("visibilitychange", refreshOnVisible);

    return () => {
      window.removeEventListener("focus", refreshSheetCount);
      window.removeEventListener("storage", refreshSheetCount);
      window.removeEventListener(SHEET_UPDATED_EVENT, refreshSheetCount);
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

  const commitSearch = useCallback((rawQuery: string, options: { scroll?: boolean; view?: "cards" | "sheet" } = {}) => {
    const q = rawQuery.trim();
    const nextParams = new URLSearchParams();
    if (options.view === "sheet") nextParams.set("view", "sheet");
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
      shouldScrollToResultsRef.current = options.scroll ?? options.view === "sheet";
      setSearchParams(nextParams);
    }
  }, [setSearchParams]);

  const handleInlineSearchCommit = useCallback((q: string) => {
    addRecentSearch(q);
    setRecents(getRecentSearches());
  }, []);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    commitSearch(query, { view: "cards" });
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
  const featured = properties.length > 0 ? pickFeatured(properties) : null;

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
          Property discovery that explains why, not just what.
          Every listing comes with context you can trust.
        </p>

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
            <button
              type="button"
              className={`home-saved-shortcut ${activeView === "sheet" ? "home-saved-shortcut--active" : ""}`}
              onClick={() => commitSearch("", { view: "sheet", scroll: true })}
              aria-pressed={activeView === "sheet"}
              aria-label={sheetCount > 0 ? `Open saved homes, ${sheetCount} saved` : "Open saved homes"}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
              </svg>
              <span>Saved</span>
              {sheetCount > 0 && <strong>{sheetCount}</strong>}
            </button>
          </div>
        </form>

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

        {/* Trending strip */}
        {snapshot && snapshot.trending.length > 0 && (
          <div
            className="fade-up fade-up-delay-4 home-trending-strip"
          >
            <span
              style={{
                fontSize: "0.68rem",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "#999",
                fontWeight: 700,
                whiteSpace: "nowrap",
                padding: "0.4rem 0.75rem 0.4rem 0",
              }}
            >
              Area Tracker
            </span>
            {snapshot.trending.slice(0, 4).map((t, i, items) => (
              <button
                key={t.label}
                type="button"
                className={`home-trending-btn${i < items.length - 1 ? " home-trending-btn--divider" : ""}`}
                onClick={() => {
                  commitSearch(t.searchQuery);
                }}
              >
                <span className="home-trending-label">{t.label}</span>
                <span className="home-trending-value">{t.value}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      {hasInlinePane && (
        <section ref={inlineResultsRef} className="home-inline-results-anchor" aria-label="Search results">
          <Suspense
            fallback={
              <div className="inline-results-shell">
                <div className="inline-results-header">
                  <span className="inline-results-kicker">
                    {activeView === "sheet" && !activeSearchQuery ? "Saved homes" : "Search results"}
                  </span>
                  <div className="results-view-switch" aria-hidden="true">
                    <span className={activeView === "cards" ? "results-view-switch-btn results-view-switch-btn--active" : "results-view-switch-btn"}>Results</span>
                    <span className={activeView === "sheet" ? "results-view-switch-btn results-view-switch-btn--active" : "results-view-switch-btn"}>Saved</span>
                  </div>
                  <div className="skeleton-search-bar skeleton-bar" />
                </div>
              </div>
            }
          >
            <InlineSearchExperience
              variant="embedded"
              onSearchCommit={handleInlineSearchCommit}
            />
          </Suspense>
        </section>
      )}

      {/* Micro-market intelligence cards */}
      {snapshot && properties.length > 0 && (
        <MicroMarketsSection properties={properties} onSearch={commitSearch} />
      )}

      {/* Market Pulse — real data snapshot */}
      {snapshot && (
        <section
          ref={pulseRef}
          className="home-pulse-section"
          style={{
            padding: "clamp(3rem, 6vw, 5rem) clamp(1.5rem, 4vw, 4rem)",
          }}
        >
          <div
            style={{
              maxWidth: "960px",
              margin: "0 auto",
              opacity: pulseVisible ? 1 : 0,
              transform: pulseVisible ? "translateY(0)" : "translateY(24px)",
              transition: "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
            }}
          >
            <h2 className="home-pulse-heading">Market pulse</h2>
            <p style={{ color: "#888", marginBottom: "2rem", fontSize: "0.95rem" }}>
              Live snapshot across {snapshot.totalAreas} micro-markets in Bengaluru
            </p>

            <div className="home-pulse-grid">
              {/* Price range + BHK breakdown */}
              <div className="home-pulse-card home-pulse-card--clay">
                <p className="home-pulse-label">Price range</p>
                <div style={{ display: "flex", alignItems: "baseline", gap: "0.5rem", marginBottom: "1rem" }}>
                  <span className="home-pulse-metric home-pulse-metric--clay">
                    {formatPrice(snapshot.priceMin)}
                  </span>
                  <span style={{ color: "#ccc" }}>&mdash;</span>
                  <span className="home-pulse-metric home-pulse-metric--sage">
                    {formatPrice(snapshot.priceMax)}
                  </span>
                </div>
                <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
                  {Object.entries(snapshot.bhkBreakdown)
                    .sort(([a], [b]) => Number(a) - Number(b))
                    .map(([bhk, count]) => (
                      <button
                        key={bhk}
                        type="button"
                        className="home-pulse-bhk-btn"
                        onClick={() => commitSearch(`${bhk}BHK`)}
                      >
                        {bhk} BHK <span style={{ color: "#aaa", marginLeft: "0.25rem" }}>{count}</span>
                      </button>
                    ))}
                </div>
              </div>

              {/* Top builders */}
              <div className="home-pulse-card home-pulse-card--sage">
                <p className="home-pulse-label">Top builders</p>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
                  {snapshot.topBuilders.map((b) => (
                    <button
                      key={b.name}
                      type="button"
                      className="home-pulse-builder-btn"
                      onClick={() => commitSearch(b.name)}
                    >
                      <span style={{ fontWeight: 500 }}>{b.name}</span>
                      <span style={{ fontSize: "0.75rem", color: "#999" }}>{b.count} listings</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Featured property preview — real data */}
              {featured && (
                <Link
                  to={`/property/${featured.id}`}
                  style={{ textDecoration: "none", color: "inherit" }}
                >
                  <div className="home-pulse-featured home-pulse-card--highlight">
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.75rem" }}>
                      <p style={{ margin: 0, fontSize: "0.75rem", color: "#999", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                        Sample transparency report
                      </p>
                      <span className="home-pulse-live">Live</span>
                    </div>
                    <p style={{ fontSize: "0.95rem", fontWeight: 600, margin: "0 0 0.15rem", color: "#1a1a1a" }}>
                      {featured.title}
                    </p>
                    <p style={{ fontSize: "0.78rem", color: "#999", margin: "0 0 0.75rem" }}>
                      {featured.area}, Bengaluru
                    </p>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.5rem" }}>
                      <span style={{ fontSize: "0.85rem", color: "#444" }}>Price</span>
                      <span style={{ fontSize: "1.1rem", fontWeight: 700, color: "#b85a3c" }}>{formatPrice(featured.price)}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.75rem" }}>
                      <span style={{ fontSize: "0.85rem", color: "#444" }}>Per sqft</span>
                      <span style={{ fontSize: "0.95rem", fontWeight: 600, color: "#888" }}>
                        {featured.price_per_sqft.toLocaleString("en-IN")} /sqft
                      </span>
                    </div>
                    {featured.transparency_tags.length > 0 && (
                      <div style={{ display: "flex", gap: "0.4rem", flexWrap: "wrap", marginTop: "auto" }}>
                        {featured.transparency_tags.slice(0, 3).map((tag) => {
                          const isSellerRegistered = tag === "seller-registered";
                          const isVerificationPending = tag === "verification-pending";
                          return (
                            <span
                              key={tag}
                              style={{
                                fontSize: "0.7rem",
                                padding: "0.2rem 0.55rem",
                                borderRadius: "999px",
                                backgroundColor: isSellerRegistered
                                  ? "rgba(251, 191, 36, 0.15)"
                                  : isVerificationPending
                                  ? "rgba(156, 163, 175, 0.15)"
                                  : "var(--color-positive-bg)",
                                color: isSellerRegistered
                                  ? "#92400e"
                                  : isVerificationPending
                                  ? "#6b7280"
                                  : "var(--color-positive)",
                                fontWeight: 500,
                              }}
                            >
                              {tag.replace(/-/g, " ")}
                            </span>
                          );
                        })}
                      </div>
                    )}
                  </div>
                </Link>
              )}
            </div>
          </div>
        </section>
      )}

      {/* Explore Bengaluru section removed — Area Price Strip covers this better */}
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
  priceMin: number;
  priceMax: number;
  count: number;
  bhks: number[];
  readyToMove: number;
  nearMetro: number;
  topBuilder: string;
  avgRating: number | null;
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
      const prices = ps.map((p) => p.price_per_sqft);
      const bhkSet = new Set(ps.map((p) => p.bhk));
      const builderCount: Record<string, number> = {};
      for (const p of ps) {
        builderCount[p.builder_name] = (builderCount[p.builder_name] ?? 0) + 1;
      }
      const topBuilder = Object.entries(builderCount).sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
      const ratings = ps.filter((p) => p.google_rating && p.google_rating > 0).map((p) => p.google_rating!);
      const societies = new Set(ps.map((p) => p.society_name));

      return {
        area,
        vibe: AREA_VIBES[area] ?? "",
        avgPriceSqft: Math.round(prices.reduce((a, b) => a + b, 0) / prices.length),
        priceMin: Math.min(...ps.map((p) => p.price)),
        priceMax: Math.max(...ps.map((p) => p.price)),
        count: ps.length,
        bhks: Array.from(bhkSet).sort(),
        readyToMove: ps.filter((p) => p.possession_status === "ready").length,
        nearMetro: ps.filter((p) => p.metro_distance_mins <= 15).length,
        topBuilder,
        avgRating: ratings.length > 0 ? Math.round((ratings.reduce((a, b) => a + b, 0) / ratings.length) * 10) / 10 : null,
        societies: societies.size,
      };
    })
    .sort((a, b) => b.count - a.count);
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
  const pct = (m.avgPriceSqft / maxAvg) * 100;
  const barColor =
    pct > 85 ? "var(--color-accent)" : pct > 60 ? "var(--color-sage-deep)" : "var(--color-sage)";

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

      {/* Price bar */}
      <div style={{ marginBottom: "0.85rem" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.3rem" }}>
          <span className="home-micro-price">
            {m.avgPriceSqft.toLocaleString("en-IN")}
            <span style={{ fontSize: "0.7rem", color: "#999", fontWeight: 400, marginLeft: "0.2rem" }}>/sqft</span>
          </span>
          <span style={{ fontSize: "0.72rem", color: "#aaa" }}>
            {formatPrice(m.priceMin)} – {formatPrice(m.priceMax)}
          </span>
        </div>
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
      </div>

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
          <span className="home-chip-sage">
            {m.nearMetro} near metro
          </span>
        )}
        {m.avgRating !== null && (
          <span style={chipStyle("rgba(218,165,32,0.1)", "#8a6d00")}>
            ★ {m.avgRating}
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

function MicroMarketsSection({
  properties,
  onSearch,
}: {
  properties: PropertyCard[];
  onSearch: (query: string) => void;
}) {
  const markets = deriveMicroMarkets(properties);
  if (markets.length < 2) return null;

  const maxAvg = Math.max(...markets.map((m) => m.avgPriceSqft));

  return (
    <section className="home-micro-section" style={{ padding: "2.5rem clamp(1.5rem, 4vw, 4rem) 2rem" }}>
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
