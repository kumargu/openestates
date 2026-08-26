/**
 * Inline discovery results with backend search integration.
 * In local development, the API layer can serve checked-in fixtures when the
 * Rust backend is unavailable so product review does not render a blank shell.
 */
import { useEffect, useState, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type {
  PropertyCard as PropertyCardType,
  SearchResponse,
  SearchAreaContext,
  MatchExplanation,
  SearchResultItem,
} from "../lib/types.ts";
import { getProperties, searchProperties } from "../lib/api.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import { primaryProofFocus } from "../lib/proof-focus.ts";
import {
  searchResultsAnnouncement,
  searchResultReasonLabels,
  type MatchResult,
} from "../lib/search.ts";
import { PageState } from "../components/PageState.tsx";
import { PropertySidePanel } from "../components/PropertySidePanel.tsx";
import { addRecentSearch } from "../lib/recent-searches.ts";
import { LivingEvidenceTile } from "../components/evidence/LivingEvidenceTile.tsx";
import { SearchFocusBoard } from "../components/evidence/SearchFocusBoard.tsx";
import { UniverseBoard } from "../components/evidence/UniverseBoard.tsx";
import { useEvidenceBatch } from "../hooks/useEvidenceBatch.ts";

/* ---------- Area Context Bar ---------- */

function isEnriched(text: string | undefined): boolean {
  if (!text) return false;
  return !text.toLowerCase().includes("not yet enriched") && !text.toLowerCase().includes("not assessed");
}

// Extract a compact signal from verbose summary text
function extractSignal(text: string): string {
  // Take first sentence only, cap at 60 chars
  const first = text.split(/\.\s/)[0].replace(/\.$/, "");
  if (first.length <= 60) return first;
  return first.slice(0, 57).replace(/\s+\S*$/, "") + "...";
}

type AreaSignal = {
  icon: string;
  label: string;
  detail: string;
  sentiment: "positive" | "neutral" | "caution";
};

function deriveAreaSignals(ctx: SearchAreaContext): AreaSignal[] {
  const signals: AreaSignal[] = [];

  // Metro
  if (isEnriched(ctx.metro_access_summary)) {
    const text = ctx.metro_access_summary.toLowerCase();
    const hasMetro = text.includes("operational") || text.includes("metro station");
    signals.push({
      icon: "\u{1F687}",
      label: hasMetro ? "Metro access" : "Metro planned",
      detail: extractSignal(ctx.metro_access_summary),
      sentiment: hasMetro ? "positive" : "neutral",
    });
  }

  // Traffic
  if (isEnriched(ctx.traffic_summary)) {
    const text = ctx.traffic_summary.toLowerCase();
    const severe = text.includes("severe") || text.includes("notorious") || text.includes("heavy");
    signals.push({
      icon: "\u{1F697}",
      label: severe ? "Heavy traffic" : "Moderate traffic",
      detail: extractSignal(ctx.traffic_summary),
      sentiment: severe ? "caution" : "neutral",
    });
  }

  // Waterlogging
  if (isEnriched(ctx.waterlogging_summary)) {
    const text = ctx.waterlogging_summary.toLowerCase();
    const risk = text.includes("waterlogging") || text.includes("flooding");
    if (risk) {
      signals.push({
        icon: "\u{1F4A7}",
        label: "Waterlogging risk",
        detail: extractSignal(ctx.waterlogging_summary),
        sentiment: "caution",
      });
    }
  }

  // Livability
  if (isEnriched(ctx.livability_summary)) {
    signals.push({
      icon: "\u{2728}",
      label: "Livability",
      detail: extractSignal(ctx.livability_summary),
      sentiment: "positive",
    });
  }

  return signals;
}

const SENTIMENT_STYLES: Record<string, { bg: string; color: string; border: string }> = {
  positive: { bg: "rgba(42,122,42,0.06)", color: "#2a7a2a", border: "rgba(42,122,42,0.12)" },
  neutral: { bg: "rgba(0,0,0,0.03)", color: "#555", border: "rgba(0,0,0,0.06)" },
  caution: { bg: "rgba(180,100,20,0.06)", color: "#8a6d00", border: "rgba(180,100,20,0.12)" },
};

function AreaContextBar({ ctx }: { ctx: SearchAreaContext }) {
  const signals = deriveAreaSignals(ctx);
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // Collect externality tags, skip duplicates with signals
  const tags = (ctx.externality_tags ?? []).slice(0, 5);

  return (
    <div
      className="section-card"
      style={{ padding: "0.85rem 1.25rem", marginBottom: "1.25rem" }}
    >
      {/* Header row: area name + price + trend */}
      <div style={{ display: "flex", alignItems: "center", gap: "0.6rem", marginBottom: signals.length > 0 || tags.length > 0 ? "0.65rem" : 0, flexWrap: "wrap" }}>
        <h2 style={{ margin: 0, fontSize: "1rem", fontWeight: 600, color: "var(--color-text)", letterSpacing: "-0.01em" }}>
          {ctx.name}
        </h2>
        {ctx.median_price_per_sqft > 0 && (
          <span style={{
            fontSize: "0.82rem",
            fontWeight: 600,
            color: "var(--color-text)",
            padding: "0.15rem 0.5rem",
            borderRadius: "6px",
            backgroundColor: "rgba(0,0,0,0.04)",
          }}>
            {ctx.median_price_per_sqft.toLocaleString("en-IN")} /sqft
          </span>
        )}
        {ctx.trend_direction && isEnriched(ctx.trend_direction) && (
          <span style={{
            fontSize: "0.75rem",
            fontWeight: 500,
            color: ctx.trend_direction === "up" ? "#2a7a2a" : ctx.trend_direction === "down" ? "#b33" : "#888",
          }}>
            {ctx.trend_direction === "up" ? "\u2197" : ctx.trend_direction === "down" ? "\u2198" : "\u2192"} {ctx.trend_direction}
          </span>
        )}
        {ctx.community_notes && isEnriched(ctx.community_notes) && (
          <span style={{ fontSize: "0.8rem", color: "var(--color-text-muted)", fontStyle: "italic" }}>
            {ctx.community_notes.length > 80 ? ctx.community_notes.slice(0, 77).replace(/\s+\S*$/, "") + "..." : ctx.community_notes}
          </span>
        )}
      </div>

      {/* Signal chips — scannable, no paragraphs */}
      {signals.length > 0 && (
        <div style={{ display: "flex", gap: "0.4rem", flexWrap: "wrap", marginBottom: tags.length > 0 ? "0.5rem" : 0 }}>
          {signals.map((s, i) => {
            const style = SENTIMENT_STYLES[s.sentiment];
            const isHovered = hoveredIdx === i;
            return (
              <div
                key={s.label}
                onMouseEnter={() => setHoveredIdx(i)}
                onMouseLeave={() => setHoveredIdx(null)}
                style={{
                  position: "relative",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "0.35rem",
                  padding: "0.3rem 0.65rem",
                  borderRadius: "8px",
                  fontSize: "0.78rem",
                  fontWeight: 500,
                  backgroundColor: style.bg,
                  color: style.color,
                  border: `1px solid ${style.border}`,
                  cursor: "default",
                  transition: "box-shadow 0.15s",
                  boxShadow: isHovered ? "0 2px 8px rgba(0,0,0,0.08)" : "none",
                }}
              >
                <span style={{ fontSize: "0.85rem" }}>{s.icon}</span>
                {s.label}
                {/* Tooltip on hover */}
                {isHovered && (
                  <div style={{
                    position: "absolute",
                    bottom: "calc(100% + 6px)",
                    left: "50%",
                    transform: "translateX(-50%)",
                    padding: "0.5rem 0.75rem",
                    borderRadius: "8px",
                    backgroundColor: "#1a1a1a",
                    color: "#eee",
                    fontSize: "0.75rem",
                    lineHeight: 1.4,
                    whiteSpace: "nowrap",
                    maxWidth: "280px",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    zIndex: 10,
                    pointerEvents: "none",
                    boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
                  }}>
                    {s.detail}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Infrastructure/externality tags */}
      {tags.length > 0 && (
        <div style={{ display: "flex", gap: "0.3rem", flexWrap: "wrap" }}>
          {tags.map((t) => (
            <span key={t} className="tag tag-neutral" style={{ fontSize: "0.7rem" }}>{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

/* ---------- Landing search experience ---------- */

type SearchExperienceProps = {
  onSearchCommit?: (query: string) => void;
  onResultsReady?: () => void;
};

export function SearchExperience({ onSearchCommit, onResultsReady }: SearchExperienceProps) {
  const [properties, setProperties] = useState<PropertyCardType[]>([]);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  const [searchResponse, setSearchResponse] = useState<SearchResponse | null>(null);
  const [searchFailed, setSearchFailed] = useState(false);
  const [panelPropertyId, setPanelPropertyId] = useState<string | null>(null);
  const [retryKey, setRetryKey] = useState(0);

  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("q") || "";
  const areaFilter = searchParams.get("area") || "";

  useEffect(() => {
    if (!query || status === "loading") return;
    onResultsReady?.();
  }, [onResultsReady, query, status]);

  // When there's a search query, call the backend search API.
  // When there's no query, load all properties.
  // The API layer owns the development fixture fallback when the backend is down.
  useEffect(() => {
    let cancelled = false;

    queueMicrotask(() => {
      if (cancelled) return;
      setStatus("loading");
      setSearchResponse(null);
      setSearchFailed(false);
    });

    if (query) {
      addRecentSearch(query);
      onSearchCommit?.(query);
      searchProperties(query)
        .then((data) => {
          if (cancelled) return;
          setSearchResponse(data);
          setStatus("ok");
        })
        .catch(() => {
          if (cancelled) return;
          setSearchFailed(true);
          setStatus("error");
        });
    } else {
      getProperties()
        .then((data) => {
          if (cancelled) return;
          setProperties(data);
          setStatus("ok");
        })
        .catch(() => {
          if (!cancelled) setStatus("error");
        });
    }
    return () => {
      cancelled = true;
    };
  }, [query, onSearchCommit, retryKey]);

  const hasQuery = query.trim().length > 0;
  const useBackendResults = hasQuery && searchResponse !== null && !searchFailed;
  const waitingForSearchResults = hasQuery && searchResponse === null && !searchFailed;

  // Apply area filter client-side (from homepage area card clicks)
  const filtered = useMemo(() => {
    if (!areaFilter) return properties;
    const filter = areaFilter.toLowerCase();
    return properties.filter((p) => p.area.toLowerCase().includes(filter));
  }, [properties, areaFilter]);

  const matchResults: { property: PropertyCardType; match?: MatchResult; explanation?: MatchExplanation }[] = useMemo(() => {
    if (useBackendResults && searchResponse) {
      const cards = searchResponse.resultSets.flatMap((set) => set.results);
      return cards.map((r) => ({
        property: r as PropertyCardType,
        match: {
          label: r.match_label as MatchResult["label"],
          reason: r.match_reason,
        },
        explanation: r.match_explanation,
      }));
    }
    if (hasQuery) return [];
    // No query — show all properties without match labels
    return filtered.map((p) => ({ property: p }));
  }, [hasQuery, useBackendResults, searchResponse, filtered]);

  const propertiesById = useMemo(() => {
    const next = new Map<string, PropertyCardType>();
    for (const property of properties) next.set(property.id, property);
    for (const { property } of matchResults) next.set(property.id, property);
    return next;
  }, [matchResults, properties]);

  const universeResults: SearchResultItem[] = useMemo(() => {
    if (useBackendResults && searchResponse) {
      return searchResponse.resultSets.flatMap((set) => set.results);
    }
    if (hasQuery) return [];
    return filtered.map((property) => ({
      ...property,
      match_score: 0,
      match_label: "Browse",
      match_reason: "In catalog",
      match_tier: "supported",
    }));
  }, [hasQuery, useBackendResults, searchResponse, filtered]);

  const propertyIds = useMemo(() => {
    return universeResults.map((result) => result.id);
  }, [universeResults]);
  const { byId: evidenceById } = useEvidenceBatch(propertyIds, propertyIds.length > 0);

  const areaContext: SearchAreaContext | null = useBackendResults ? searchResponse.areaContext ?? null : null;
  const totalCount = useBackendResults ? searchResponse.totalMatches : hasQuery ? 0 : filtered.length;
  const returnedCount = totalCount;
  const searchGuidance = useBackendResults ? searchResponse.searchGuidance : undefined;
  const containerClass = "inline-results-shell";

  if (status === "loading") return (
    <div className={containerClass}>
      <div className="results-grid">
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="skeleton-card">
            <div className="skeleton-card-image skeleton-bar" />
            <div className="skeleton-card-body">
              <div className="skeleton-card-title skeleton-bar" />
              <div className="skeleton-card-location skeleton-bar" />
              <div className="skeleton-card-price skeleton-bar" />
              <div className="skeleton-card-tags">
                <div className="skeleton-card-tag skeleton-bar" />
                <div className="skeleton-card-tag skeleton-bar" />
                <div className="skeleton-card-tag skeleton-bar" />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
  if (status === "error") {
    return (
      <div className={containerClass}>
        <PageState
          variant="error"
          context="results"
          onRetry={() => setRetryKey((current) => current + 1)}
        />
      </div>
    );
  }

  const helmetTitle = query
    ? `${query} — Explore | ${PUBLIC_BRAND_NAME}`
    : `Explore | ${PUBLIC_BRAND_NAME}`;
  const helmetDescription = query
    ? `${totalCount} ${totalCount === 1 ? "property" : "properties"} matching "${query}".`
    : `Browse ${totalCount} proof-backed homes on ${PUBLIC_BRAND_NAME}.`;

  const renderTile = (result: SearchResultItem) => (
    <LivingEvidenceTile
      property={result}
      onQuickView={setPanelPropertyId}
      matchLabels={hasQuery ? searchResultReasonLabels(result) : []}
      proofFocus={primaryProofFocus(result, query)}
    />
  );
  return (
    <div className={containerClass}>
      <Helmet>
        <title>{helmetTitle}</title>
        <meta name="description" content={helmetDescription} />
        <meta property="og:title" content={helmetTitle} />
        <meta property="og:description" content={helmetDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content={PUBLIC_BRAND_NAME} />
      </Helmet>

      {/* Accessible live region — announces result count to screen readers */}
      <div aria-live="polite" className="sr-only">
        {query
          ? searchResultsAnnouncement(
            query,
            totalCount,
            returnedCount,
          )
          : `Showing ${totalCount} ${totalCount === 1 ? "property" : "properties"}.`}
      </div>

      {/* Area context bar — shown when backend search returns area info */}
      {areaContext && <AreaContextBar ctx={areaContext} />}

      {matchResults.length === 0 && query && !waitingForSearchResults && (
        <div className="empty-state">
          <h2>{searchGuidance?.title ?? "No homes match this search."}</h2>
          {searchGuidance?.message && <p>{searchGuidance.message}</p>}
          {searchGuidance && searchGuidance.suggestions.length > 0 && (
            <div className="empty-state-chips">
              {searchGuidance.suggestions.map((suggestion) => (
                <button
                  key={suggestion}
                  type="button"
                  className="empty-state-chip"
                  onClick={() => setSearchParams({ q: suggestion })}
                >
                  {suggestion}
                </button>
              ))}
            </div>
          )}
          <button
            type="button"
            className="inline-results-clear"
            onClick={() => setSearchParams({})}
          >
            Clear search
          </button>
        </div>
      )}

      {waitingForSearchResults ? (
        <div className="results-grid" aria-label="Loading search results">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="skeleton-card">
              <div className="skeleton-card-image skeleton-bar" />
              <div className="skeleton-card-body">
                <div className="skeleton-card-title skeleton-bar" />
                <div className="skeleton-card-location skeleton-bar" />
                <div className="skeleton-card-price skeleton-bar" />
                <div className="skeleton-card-tags">
                  <div className="skeleton-card-tag skeleton-bar" />
                  <div className="skeleton-card-tag skeleton-bar" />
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : useBackendResults && searchResponse ? (
        <SearchFocusBoard
          resultSets={searchResponse.resultSets}
          renderResult={renderTile}
        />
      ) : (
        <UniverseBoard
          results={universeResults}
          evidenceById={evidenceById}
          renderResult={renderTile}
        />
      )}

      {/* Side panel — quick view */}
      {panelPropertyId && (() => {
        const panelCard = matchResults.find(r => r.property.id === panelPropertyId)?.property ?? propertiesById.get(panelPropertyId);
        if (!panelCard) return null;
        return (
          <PropertySidePanel
            propertyId={panelPropertyId}
            card={panelCard}
            onClose={() => setPanelPropertyId(null)}
          />
        );
      })()}
    </div>
  );
}
