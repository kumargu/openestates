import { useCallback, useEffect, useState, useRef } from "react";
import { useSearchParams } from "react-router-dom";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";
import { getAreaTracker, getProperties } from "../lib/api.ts";
import { getRecentSearches, addRecentSearch, clearRecentSearches } from "../lib/recent-searches.ts";
import { SearchExperience as InlineSearchExperience } from "./SearchExperience.tsx";
import { AreaTrackerSection } from "../components/AreaTrackerSection.tsx";
import { LandingStoryStage } from "../components/LandingStoryStage.tsx";

const HERO_PROMISE = "Tell us the life you want. We'll show homes with receipts.";

const ROTATING_WORDS = [
  "proof you can trust",
  "known risks",
  "price context",
  "clear tradeoffs",
];

function RotatingText() {
  const [index, setIndex] = useState(0);
  const [fading, setFading] = useState(false);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) return undefined;
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
    <span className={`home-hero__rotating${fading ? " home-hero__rotating--fading" : ""}`}>
      {ROTATING_WORDS[index]}
    </span>
  );
}

/* Rotating example queries — Tab accepts the suggestion like Google/Gmail. */
const SEARCH_EXAMPLES = [
  "Quiet 3BHK near good schools under 2.5Cr",
  "Family-friendly society in Sarjapur with metro access",
  "Ready-to-move 2BHK in HSR with a strong builder record",
  "Low commute-pain home near Whitefield tech parks under 1.8Cr",
  "Premium 4BHK in Hebbal with RERA proof and low traffic",
  "Value flat with good resale near ORR, delivered on time",
];

const SEARCH_SUGGESTIONS = [
  { label: "Low commute", query: "Low commute-pain home near Whitefield tech parks" },
  { label: "Family-friendly", query: "Family-friendly 3BHK near good schools" },
  { label: "Strong proof", query: "Homes with strong RERA proof and good Google reviews" },
  { label: "Under 2.5Cr", query: "3BHK under 2.5Cr with good resale" },
];

const GHOST_ROTATE_MS = 3400;
const GHOST_FADE_MS = 400;

export function HomePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeSearchQuery = searchParams.get("q") || "";
  const hasActiveSearch = activeSearchQuery.trim().length > 0;
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [areaTracker, setAreaTracker] = useState<AreaTrackerResponse | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState(activeSearchQuery);
  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
  const [searchFocused, setSearchFocused] = useState(false);
  const [exampleIndex, setExampleIndex] = useState(0);
  const [ghostFading, setGhostFading] = useState(false);
  const shouldSettleSearchRef = useRef(false);

  const showGhost = !query;
  useEffect(() => {
    if (!showGhost) {
      setGhostFading(false);
      return undefined;
    }
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (media.matches) return undefined;
    const interval = window.setInterval(() => {
      setGhostFading(true);
      window.setTimeout(() => {
        setExampleIndex((i) => (i + 1) % SEARCH_EXAMPLES.length);
        setGhostFading(false);
      }, GHOST_FADE_MS);
    }, GHOST_ROTATE_MS);
    return () => window.clearInterval(interval);
  }, [showGhost]);

  useEffect(() => {
    if (searchParams.get("view") === "saved") {
      setSearchParams({}, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    const timer = window.setTimeout(() => {
      getProperties({ signal: controller.signal })
        .then((props) => {
          if (cancelled) return;
          setProperties(props);
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
    }, 750);
    return () => {
      cancelled = true;
      controller.abort();
      window.clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    setQuery(activeSearchQuery);
  }, [activeSearchQuery]);

  // Airbnb-style: settle at the compact search chrome, don't jump to a "new page".
  useEffect(() => {
    if (!hasActiveSearch || !shouldSettleSearchRef.current) return;
    shouldSettleSearchRef.current = false;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    window.scrollTo({ top: 0, behavior: media.matches ? "auto" : "smooth" });
  }, [activeSearchQuery, hasActiveSearch]);

  const commitSearch = useCallback((rawQuery: string, options: { settle?: boolean } = {}) => {
    const q = rawQuery.trim();
    const nextParams = new URLSearchParams();
    setQuery(q);
    if (q) {
      sessionStorage.setItem("oe_search_query", q);
      addRecentSearch(q);
      setRecents(getRecentSearches());
      shouldSettleSearchRef.current = options.settle ?? true;
      nextParams.set("q", q);
      setSearchParams(nextParams);
    } else {
      sessionStorage.removeItem("oe_search_query");
      shouldSettleSearchRef.current = false;
      setSearchParams(nextParams);
    }
  }, [setSearchParams]);

  const clearSearch = useCallback(() => {
    commitSearch("", { settle: false });
  }, [commitSearch]);

  const handleInlineSearchCommit = useCallback((q: string) => {
    addRecentSearch(q);
    setRecents(getRecentSearches());
  }, []);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    commitSearch(query);
  };

  return (
    <div className={`home-page${hasActiveSearch ? " home-page--searching" : ""}`}>
      <section
        className={`home-hero${hasActiveSearch ? " home-hero--search-active" : ""}`}
        aria-label={hasActiveSearch ? "Search" : "Home"}
      >
        <div className="home-hero__wash" aria-hidden="true" />

        {!hasActiveSearch && (
          <>
            <div className="fade-up home-hero__copy">
              <h1 className="home-hero__title">
                Discover{" "}
                <RotatingText />
              </h1>
            </div>

            <p className="fade-up fade-up-delay-1 home-hero__promise">
              {HERO_PROMISE}
            </p>
          </>
        )}

        {hasActiveSearch && (
          <div className="home-hero__search-bar">
            <button
              type="button"
              className="home-hero__exit"
              onClick={clearSearch}
            >
              Clear search
            </button>
          </div>
        )}

        <form
          onSubmit={handleSearch}
          className={`home-composer${hasActiveSearch ? "" : " fade-up fade-up-delay-1"}${searchFocused ? " home-composer--focused" : ""}`}
          aria-label="Property search"
          role="search"
        >
          <div className="home-composer__field">
            <svg className="home-composer__lead" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              className="home-composer__input"
              type="text"
              placeholder={showGhost ? "" : "Area, BHK, budget, commute, schools, vibe…"}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onFocus={() => setSearchFocused(true)}
              onBlur={() => setSearchFocused(false)}
              onKeyDown={(e) => {
                if (e.key !== "Tab" || e.shiftKey || query.trim()) return;
                e.preventDefault();
                commitSearch(SEARCH_EXAMPLES[exampleIndex]);
              }}
              aria-label="Describe the property you are looking for"
              aria-describedby={showGhost ? "home-search-ghost-hint" : undefined}
            />
            {showGhost && (
              <span
                className={`home-composer__ghost${ghostFading ? " home-composer__ghost--fading" : ""}`}
                aria-hidden="true"
              >
                <span className="home-composer__ghost-text">{SEARCH_EXAMPLES[exampleIndex]}</span>
                <kbd className="home-composer__tab-hint">Tab</kbd>
              </span>
            )}
            {showGhost && (
              <span id="home-search-ghost-hint" className="sr-only">
                Press Tab to search with the suggested query.
              </span>
            )}
            {hasActiveSearch && query.trim() && (
              <button
                type="button"
                className="home-composer__clear"
                aria-label="Clear search"
                onClick={clearSearch}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            )}
            <button type="submit" className="home-composer__submit" aria-label="Search">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M5 12h14M13 6l6 6-6 6" />
              </svg>
            </button>
          </div>
        </form>

        <div
          className={`home-search-suggestions${hasActiveSearch ? "" : " fade-up fade-up-delay-2"}`}
          aria-label="Suggested searches"
        >
          {SEARCH_SUGGESTIONS.map((suggestion) => (
            <button
              key={suggestion.label}
              type="button"
              className={`home-search-suggestion${activeSearchQuery === suggestion.query ? " is-active" : ""}`}
              onClick={() => commitSearch(suggestion.query)}
            >
              {suggestion.label}
            </button>
          ))}
        </div>

        {loadError && (
          <div className={`home-error-banner${hasActiveSearch ? "" : " fade-up fade-up-delay-2"}`}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#92400e" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
            <span>Market data temporarily unavailable. Search still works.</span>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="home-error-banner__retry"
            >
              Retry
            </button>
          </div>
        )}

        {!hasActiveSearch && recents.length > 0 && (
          <div className="fade-up fade-up-delay-3 recent-searches">
            <span className="recent-searches-label">Recent</span>
            {recents.map((s) => (
              <button
                key={s}
                type="button"
                className="empty-state-chip"
                onClick={() => {
                  commitSearch(s);
                }}
              >
                {s}
              </button>
            ))}
            <button
              type="button"
              className="recent-clear-btn"
              onClick={() => { clearRecentSearches(); setRecents([]); }}
            >
              clear
            </button>
          </div>
        )}
      </section>

      <div className="home-body" aria-live="polite">
        {hasActiveSearch ? (
          <section className="home-inline-results-anchor" aria-label="Search results">
            <InlineSearchExperience
              onSearchCommit={handleInlineSearchCommit}
            />
          </section>
        ) : (
          properties.length > 0 && (
            <>
              <LandingStoryStage properties={properties} onSearch={commitSearch} />
              <AreaTrackerSection
                properties={properties}
                areaTracker={areaTracker}
                onSearch={commitSearch}
                maxMarkets={6}
              />
            </>
          )
        )}
      </div>
    </div>
  );
}
