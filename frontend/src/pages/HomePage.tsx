import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useSearchParams } from "react-router-dom";
import type { AreaTrackerResponse, PropertyCard } from "../lib/types.ts";
import { getAreaTracker, getProperties } from "../lib/api.ts";
import { getRecentSearches, addRecentSearch, clearRecentSearches } from "../lib/recent-searches.ts";
import { SearchExperience as InlineSearchExperience } from "./SearchExperience.tsx";
import { AreaTrackerSection } from "../components/AreaTrackerSection.tsx";
import { LandingStoryStage } from "../components/LandingStoryStage.tsx";
import { consumeDiscoveryReturn } from "../lib/navigationContext.ts";

const SEARCH_SUGGESTIONS = [
  { label: "Under ₹2.5Cr", query: "3BHK under 2.5Cr with clear price context" },
  { label: "Near schools", query: "Quiet family home near good schools" },
  { label: "Ready to move", query: "Ready-to-move homes with delivery proof" },
  { label: "Whitefield", query: "Low commute-pain home near Whitefield tech parks" },
];

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => void) => { finished: Promise<void> };
};

const LOADING_CARDS = ["one", "two", "three", "four"];

function LandingLoadingState() {
  return (
    <section className="landing-loading" aria-label="Loading homes" aria-busy="true">
      <span className="landing-loading__heading" />
      <div className="landing-loading__rail">
        {LOADING_CARDS.map((card) => (
          <span key={card} className="landing-loading__card">
            <span className="landing-loading__image" />
            <span className="landing-loading__line" />
            <span className="landing-loading__line landing-loading__line--short" />
          </span>
        ))}
      </div>
    </section>
  );
}

export function HomePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeSearchQuery = searchParams.get("q") || "";
  const hasActiveSearch = activeSearchQuery.trim().length > 0;
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [areaTracker, setAreaTracker] = useState<AreaTrackerResponse | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [propertiesLoading, setPropertiesLoading] = useState(true);
  const [query, setQuery] = useState(activeSearchQuery);
  const [recents, setRecents] = useState<string[]>(() => getRecentSearches());
  const [searchFocused, setSearchFocused] = useState(false);
  const shouldSettleSearchRef = useRef(false);

  useEffect(() => {
    if (searchParams.get("view") === "saved") {
      const nextParams = new URLSearchParams(searchParams);
      nextParams.delete("view");
      setSearchParams(nextParams, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;

    Promise.allSettled([
      getProperties({ signal: controller.signal }),
      getAreaTracker({ signal: controller.signal }),
    ]).then(([propertyResult, trackerResult]) => {
      if (cancelled) return;

      if (propertyResult.status === "fulfilled") {
        setProperties(propertyResult.value);
      } else if (!(propertyResult.reason instanceof DOMException && propertyResult.reason.name === "AbortError")) {
        setLoadError(true);
      }

      if (trackerResult.status === "fulfilled") {
        setAreaTracker(trackerResult.value);
      }
      setPropertiesLoading(false);
    });

    return () => {
      cancelled = true;
      controller.abort();
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

  const applySearchParams = useCallback((params: URLSearchParams) => {
    const transitionDocument = document as ViewTransitionDocument;
    const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const isHeroInView = window.scrollY < 640;

    if (!transitionDocument.startViewTransition || prefersReducedMotion || !isHeroInView) {
      setSearchParams(params);
      return;
    }

    transitionDocument.startViewTransition(() => {
      flushSync(() => setSearchParams(params));
    });
  }, [setSearchParams]);

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
      applySearchParams(nextParams);
    } else {
      sessionStorage.removeItem("oe_search_query");
      shouldSettleSearchRef.current = false;
      applySearchParams(nextParams);
    }
  }, [applySearchParams]);

  const clearSearch = useCallback(() => {
    commitSearch("", { settle: false });
  }, [commitSearch]);

  const handleInlineSearchCommit = useCallback((q: string) => {
    addRecentSearch(q);
    setRecents(getRecentSearches());
  }, []);

  const restoreDiscoveryPosition = useCallback(() => {
    const scrollY = consumeDiscoveryReturn(`${window.location.pathname}${window.location.search}`);
    if (scrollY == null) return;
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        const root = document.documentElement;
        const previousScrollBehavior = root.style.scrollBehavior;
        root.style.scrollBehavior = "auto";
        window.scrollTo(0, scrollY);
        root.style.scrollBehavior = previousScrollBehavior;
      });
    });
  }, []);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    commitSearch(query);
  };

  return (
    <div className={`home-page${hasActiveSearch ? " home-page--searching" : ""}`}>
      <section
        className={`home-hero${hasActiveSearch ? " home-hero--search-active" : ""}`}
        aria-label="Explore"
      >
        <div className="home-hero__wash" aria-hidden="true" />

        {!hasActiveSearch && (
          <div className="fade-up home-hero__copy">
            <h1 className="home-hero__title">
              <span>Tell us the life you want.</span>
              <span>We'll show homes with receipts.</span>
            </h1>
            <p className="home-hero__promise">Fewer homes. Better reasons.</p>
          </div>
        )}
      </section>

      <div className="home-scroll-shell">
        <form
          onSubmit={handleSearch}
          className={`home-composer${hasActiveSearch ? " home-composer--search-active" : " home-composer--landing-sticky fade-up fade-up-delay-1"}${searchFocused ? " home-composer--focused" : ""}`}
          aria-label="Search homes"
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
              placeholder="Quiet 3BHK near schools under 2.5Cr"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onFocus={() => setSearchFocused(true)}
              onBlur={() => setSearchFocused(false)}
              aria-label="Describe the property you are looking for"
              autoComplete="off"
            />
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

        {!hasActiveSearch && (
          <div
            className="home-search-suggestions fade-up fade-up-delay-2"
            aria-label="Suggested searches"
          >
            {SEARCH_SUGGESTIONS.map((suggestion) => (
              <button
                key={suggestion.label}
                type="button"
                className="home-search-suggestion"
                onClick={() => commitSearch(suggestion.query)}
              >
                {suggestion.label}
              </button>
            ))}
          </div>
        )}

        {loadError && (
          <div className={`home-error-banner${hasActiveSearch ? "" : " fade-up fade-up-delay-2"}`}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#92400e" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
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
              Clear
            </button>
          </div>
        )}

        <div className="home-body" aria-live="polite">
          {hasActiveSearch ? (
            <section className="home-inline-results-anchor" aria-label="Homes matching your search">
              <InlineSearchExperience
                onSearchCommit={handleInlineSearchCommit}
                onResultsReady={restoreDiscoveryPosition}
              />
            </section>
          ) : properties.length > 0 ? (
            <>
              <LandingStoryStage properties={properties} onSearch={commitSearch} />
              <AreaTrackerSection
                properties={properties}
                areaTracker={areaTracker}
                onSearch={commitSearch}
                maxMarkets={6}
              />
            </>
          ) : propertiesLoading ? <LandingLoadingState /> : null}
        </div>
      </div>
    </div>
  );
}
