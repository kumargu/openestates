import { lazy, Suspense, useCallback, useEffect, useState, useRef } from "react";
import { Link, useSearchParams } from "react-router-dom";
import type { PropertyCard } from "../lib/types.ts";
import { getProperties, getStats, type PlatformStats } from "../lib/api.ts";
import { getRecentSearches, addRecentSearch, clearRecentSearches } from "../lib/recent-searches.ts";

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
        color: "#c96b4f",
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
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [platformStats, setPlatformStats] = useState<PlatformStats | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState(activeSearchQuery);
  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
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
    if (!hasActiveSearch || !shouldScrollToResultsRef.current) return;
    shouldScrollToResultsRef.current = false;
    window.setTimeout(() => {
      inlineResultsRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 90);
  }, [activeSearchQuery, hasActiveSearch]);

  const commitSearch = useCallback((rawQuery: string, options: { scroll?: boolean } = {}) => {
    const q = rawQuery.trim();
    setQuery(q);
    if (q) {
      sessionStorage.setItem("oe_search_query", q);
      addRecentSearch(q);
      setRecents(getRecentSearches());
      shouldScrollToResultsRef.current = options.scroll ?? true;
      setSearchParams({ q });
    } else {
      sessionStorage.removeItem("oe_search_query");
      shouldScrollToResultsRef.current = false;
      setSearchParams({});
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
  const featured = properties.length > 0 ? pickFeatured(properties) : null;

  return (
    <div>
      {/* Hero */}
      <section
        className={`home-hero ${hasActiveSearch ? "home-hero--search-active" : ""}`}
        style={{
          minHeight: hasActiveSearch ? "min(72vh, 640px)" : "96vh",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: hasActiveSearch
            ? "7rem clamp(1.5rem, 4vw, 4rem) 5rem"
            : "0 clamp(1.5rem, 4vw, 4rem)",
          position: "relative",
          overflow: "hidden",
          transition: "min-height 0.7s var(--ease-out), padding 0.7s var(--ease-out)",
        }}
      >
        <div
          style={{
            position: "absolute",
            inset: 0,
            background:
              "radial-gradient(ellipse 80% 60% at 50% 40%, rgba(201,107,79,0.06) 0%, transparent 70%), " +
              "radial-gradient(ellipse 60% 50% at 80% 20%, rgba(100,140,200,0.04) 0%, transparent 60%)",
            zIndex: -1,
          }}
        />

        <div className="fade-up" style={{ textAlign: "center", maxWidth: "720px" }}>
          <h1
            style={{
              fontSize: "clamp(2.5rem, 2rem + 3vw, 4.5rem)",
              fontWeight: 700,
              lineHeight: 1.1,
              letterSpacing: "-0.03em",
              margin: "0 0 1.5rem",
              color: "#1a1a1a",
            }}
          >
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
        <p
          className="fade-up fade-up-delay-1"
          style={{
            fontSize: "0.82rem",
            color: "#999",
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            margin: "0 0 0.75rem",
          }}
        >
          Describe what you're looking for
        </p>
        <form
          onSubmit={handleSearch}
          className="search-container home-search-form"
          aria-label="Property search"
          role="search"
        >
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
          <button
            type="submit"
            style={{
              border: "none",
              background: "#1a1a1a",
              color: "#fff",
              padding: "0.6rem 1.5rem",
              borderRadius: "10px",
              fontSize: "0.9rem",
              cursor: "pointer",
              whiteSpace: "nowrap",
              fontFamily: "inherit",
              transition: "background 0.2s ease",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "#333")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "#1a1a1a")}
          >
            Search
          </button>
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
            <span><strong style={{ color: "#555" }}>{snapshot.totalProperties}</strong> properties</span>
            <span style={{ width: 3, height: 3, borderRadius: "50%", backgroundColor: "#ccc" }} />
            <span><strong style={{ color: "#555" }}>{snapshot.totalSocieties}</strong> societies</span>
            <span style={{ width: 3, height: 3, borderRadius: "50%", backgroundColor: "#ccc" }} />
            <span><strong style={{ color: "#555" }}>{snapshot.totalAreas}</strong> micro-markets</span>
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
              onClick={() => {
                commitSearch(s);
              }}
              style={{
                border: "1px solid rgba(0,0,0,0.08)",
                background: "rgba(255,255,255,0.7)",
                color: "#666",
                padding: "0.4rem 0.85rem",
                borderRadius: "999px",
                fontSize: "0.78rem",
                cursor: "pointer",
                fontFamily: "inherit",
                transition: "all 0.2s ease",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.borderColor = "rgba(201,107,79,0.3)";
                e.currentTarget.style.color = "#c96b4f";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.borderColor = "rgba(0,0,0,0.08)";
                e.currentTarget.style.color = "#666";
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
            {snapshot.trending.slice(0, 4).map((t, i) => (
              <button
                key={t.label}
                onClick={() => {
                  commitSearch(t.searchQuery);
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  padding: "0.4rem 0.75rem",
                  borderRadius: "8px",
                  transition: "background-color 0.2s",
                  ...(i < snapshot.trending.slice(0, 4).length - 1
                    ? { borderRight: "1px solid rgba(0,0,0,0.06)", borderRadius: "8px 0 0 8px", paddingRight: "1.25rem" }
                    : {}),
                }}
                onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = "rgba(201,107,79,0.06)"; }}
                onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = "transparent"; }}
              >
                <span style={{
                  fontSize: "0.7rem",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "#c96b4f",
                  fontWeight: 600,
                  whiteSpace: "nowrap",
                }}>
                  {t.label}
                </span>
                <span style={{
                  fontSize: "0.82rem",
                  color: "#555",
                  fontWeight: 500,
                  whiteSpace: "nowrap",
                }}>
                  {t.value}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>

      {hasActiveSearch && (
        <section ref={inlineResultsRef} className="home-inline-results-anchor" aria-label="Search results">
          <Suspense
            fallback={
              <div className="inline-results-shell">
                <div className="inline-results-header">
                  <span className="inline-results-kicker">OpenEstates search</span>
                  <div className="results-view-switch" aria-hidden="true">
                    <span className="results-view-switch-btn results-view-switch-btn--active">Discover</span>
                    <span className="results-view-switch-btn">Compare</span>
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
          style={{
            padding: "clamp(3rem, 6vw, 5rem) clamp(1.5rem, 4vw, 4rem)",
            backgroundColor: "var(--color-bg-soft)",
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
            <h2
              style={{
                fontSize: "clamp(1.3rem, 1rem + 1.2vw, 2rem)",
                fontWeight: 600,
                letterSpacing: "-0.02em",
                margin: "0 0 0.5rem",
              }}
            >
              Market pulse
            </h2>
            <p style={{ color: "#888", marginBottom: "2rem", fontSize: "0.95rem" }}>
              Live snapshot across {snapshot.totalAreas} micro-markets in Bengaluru
            </p>

            <div className="home-pulse-grid">
              {/* Price range + BHK breakdown */}
              <div
                style={{
                  padding: "1.5rem",
                  borderRadius: "12px",
                  backgroundColor: "#fff",
                  border: "1px solid rgba(0,0,0,0.06)",
                }}
              >
                <p style={{ margin: "0 0 0.75rem", fontSize: "0.75rem", color: "#999", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                  Price range
                </p>
                <div style={{ display: "flex", alignItems: "baseline", gap: "0.5rem", marginBottom: "1rem" }}>
                  <span style={{ fontSize: "1.4rem", fontWeight: 700, color: "#1a1a1a" }}>
                    {formatPrice(snapshot.priceMin)}
                  </span>
                  <span style={{ color: "#ccc" }}>&mdash;</span>
                  <span style={{ fontSize: "1.4rem", fontWeight: 700, color: "#1a1a1a" }}>
                    {formatPrice(snapshot.priceMax)}
                  </span>
                </div>
                <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
                  {Object.entries(snapshot.bhkBreakdown)
                    .sort(([a], [b]) => Number(a) - Number(b))
                    .map(([bhk, count]) => (
                      <button
                        key={bhk}
                        onClick={() => commitSearch(`${bhk}BHK`)}
                        style={{
                          padding: "0.3rem 0.7rem",
                          borderRadius: "8px",
                          border: "1px solid rgba(0,0,0,0.06)",
                          background: "rgba(201,107,79,0.04)",
                          fontSize: "0.78rem",
                          cursor: "pointer",
                          fontFamily: "inherit",
                          color: "#555",
                          transition: "all 0.2s",
                        }}
                        onMouseEnter={(e) => { e.currentTarget.style.borderColor = "rgba(201,107,79,0.3)"; e.currentTarget.style.color = "#c96b4f"; }}
                        onMouseLeave={(e) => { e.currentTarget.style.borderColor = "rgba(0,0,0,0.06)"; e.currentTarget.style.color = "#555"; }}
                      >
                        {bhk} BHK <span style={{ color: "#aaa", marginLeft: "0.25rem" }}>{count}</span>
                      </button>
                    ))}
                </div>
              </div>

              {/* Top builders */}
              <div
                style={{
                  padding: "1.5rem",
                  borderRadius: "12px",
                  backgroundColor: "#fff",
                  border: "1px solid rgba(0,0,0,0.06)",
                }}
              >
                <p style={{ margin: "0 0 0.75rem", fontSize: "0.75rem", color: "#999", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                  Top builders
                </p>
                <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
                  {snapshot.topBuilders.map((b) => (
                    <button
                      key={b.name}
                      onClick={() => commitSearch(b.name)}
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        padding: "0.5rem 0.75rem",
                        borderRadius: "8px",
                        border: "1px solid rgba(0,0,0,0.04)",
                        background: "transparent",
                        cursor: "pointer",
                        fontFamily: "inherit",
                        fontSize: "0.88rem",
                        color: "#333",
                        transition: "all 0.2s",
                        textAlign: "left",
                        width: "100%",
                      }}
                      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = "rgba(201,107,79,0.04)"; e.currentTarget.style.borderColor = "rgba(201,107,79,0.15)"; }}
                      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = "transparent"; e.currentTarget.style.borderColor = "rgba(0,0,0,0.04)"; }}
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
                  <div
                    style={{
                      padding: "1.5rem",
                      borderRadius: "12px",
                      backgroundColor: "#fff",
                      border: "1px solid rgba(0,0,0,0.06)",
                      cursor: "pointer",
                      transition: "border-color 0.2s ease, box-shadow 0.2s ease",
                      height: "100%",
                      display: "flex",
                      flexDirection: "column",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.borderColor = "rgba(201,107,79,0.3)";
                      e.currentTarget.style.boxShadow = "0 2px 12px rgba(0,0,0,0.06)";
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.borderColor = "rgba(0,0,0,0.06)";
                      e.currentTarget.style.boxShadow = "none";
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.75rem" }}>
                      <p style={{ margin: 0, fontSize: "0.75rem", color: "#999", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                        Sample transparency report
                      </p>
                      <span style={{ fontSize: "0.65rem", textTransform: "uppercase", letterSpacing: "0.08em", color: "#c96b4f" }}>
                        Live
                      </span>
                    </div>
                    <p style={{ fontSize: "0.95rem", fontWeight: 600, margin: "0 0 0.15rem", color: "#1a1a1a" }}>
                      {featured.title}
                    </p>
                    <p style={{ fontSize: "0.78rem", color: "#999", margin: "0 0 0.75rem" }}>
                      {featured.area}, Bengaluru
                    </p>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.5rem" }}>
                      <span style={{ fontSize: "0.85rem", color: "#444" }}>Price</span>
                      <span style={{ fontSize: "1.1rem", fontWeight: 700 }}>{formatPrice(featured.price)}</span>
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
                                  : "rgba(42,122,42,0.08)",
                                color: isSellerRegistered
                                  ? "#92400e"
                                  : isVerificationPending
                                  ? "#6b7280"
                                  : "#2a7a2a",
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
    pct > 85 ? "#c96b4f" : pct > 60 ? "#daa520" : "#4a9a6a";

  return (
    <button
      onClick={() => onSearch(m.area)}
      style={{
        padding: "1.25rem 1.4rem",
        borderRadius: "12px",
        backgroundColor: "#fff",
        border: "1px solid rgba(0,0,0,0.06)",
        cursor: "pointer",
        fontFamily: "inherit",
        textAlign: "left",
        width: "100%",
        transition: "border-color 0.2s, box-shadow 0.2s",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = "rgba(201,107,79,0.25)";
        e.currentTarget.style.boxShadow = "0 2px 12px rgba(0,0,0,0.05)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = "rgba(0,0,0,0.06)";
        e.currentTarget.style.boxShadow = "none";
      }}
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
          <span style={{ fontSize: "1.2rem", fontWeight: 700, color: "#1a1a1a" }}>
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
          <span style={chipStyle("rgba(42,122,42,0.08)", "#2a7a2a")}>
            {m.readyToMove} ready
          </span>
        )}
        {m.nearMetro > 0 && (
          <span style={chipStyle("rgba(42,80,180,0.08)", "#2a5ab4")}>
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
    <section
      style={{
        padding: "2.5rem clamp(1.5rem, 4vw, 4rem) 2rem",
        backgroundColor: "#fff",
        borderBottom: "1px solid rgba(0,0,0,0.04)",
      }}
    >
      <div style={{ maxWidth: "960px", margin: "0 auto" }}>
        <div style={{ marginBottom: "1.5rem" }}>
          <h2 style={{ margin: "0 0 0.25rem", fontSize: "clamp(1.3rem, 1rem + 1vw, 1.8rem)", fontWeight: 600, letterSpacing: "-0.02em" }}>
            Area Tracker
          </h2>
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
