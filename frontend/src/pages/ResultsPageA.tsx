/**
 * Results page with inline subscribe actions and backend search integration.
 * All data comes from the backend API — no client-side fallbacks.
 */
import { useEffect, useState, useMemo } from "react";
import { useSearchParams, useNavigate, Link } from "react-router-dom";
import type { PropertyCard as PropertyCardType, SearchResponse, SearchAreaContext, MatchExplanation } from "../lib/types.ts";
import { getProperties, searchProperties } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { formatSearchSummary } from "../lib/search.ts";
import type { MatchResult } from "../lib/search.ts";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { PreferencePill } from "../components/PreferencePill.tsx";
import { MatchReasonBadge } from "../components/MatchReasonBadge.tsx";
import { PropertySidePanel } from "../components/PropertySidePanel.tsx";
import { CompareBar } from "../components/CompareBar.tsx";
import { ComparePanel } from "../components/ComparePanel.tsx";
import { isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";

const MAX_COMPARE = 3;

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

const LABEL_COLORS: Record<string, { bg: string; color: string; border: string }> = {
  "Strong match": { bg: "#edf7ed", color: "#2a7a2a", border: "#c8e6c8" },
  "Good match": { bg: "#f0f4ff", color: "#3b5998", border: "#c8d8f0" },
  "Value pick": { bg: "#fdf5e6", color: "#8a6d00", border: "#e8d8a0" },
  "Premium match": { bg: "#f5f0fa", color: "#6b3fa0", border: "#d8c8e8" },
};

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

/* ---------- Property Card ---------- */

function CardA({ property, match, explanation, onQuickView, isComparing, onToggleCompare }: {
  property: PropertyCardType;
  match?: MatchResult;
  explanation?: MatchExplanation;
  onQuickView?: (id: string) => void;
  isComparing?: boolean;
  onToggleCompare?: (id: string) => void;
}) {
  const [saved, setSaved] = useState(() => isShortlisted(property.id));

  const handleSave = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setSaved(toggleShortlist(property.id));
  };

  const handleQuickView = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onQuickView?.(property.id);
  };

  const handleCompare = () => {
    onToggleCompare?.(property.id);
  };

  const labelStyle = match ? LABEL_COLORS[match.label] || LABEL_COLORS["Good match"] : null;

  return (
    <div className="card-a">
      {/* Compare toggle button — outside Link to avoid invalid <button> inside <a> */}
      <button
        className={`card-a-compare-btn ${isComparing ? "card-a-compare-btn--active" : ""}`}
        onClick={handleCompare}
        title={isComparing ? "Remove from compare" : "Add to compare"}
      >
        {isComparing ? (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round"><polyline points="20 6 9 17 4 12" /></svg>
        ) : (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" /></svg>
        )}
      </button>
      <Link to={`/property/${property.id}`} className="card-a-link">
        <div className="card-a-image">
          <ImageWithFallback
            src={property.hero_image}
            alt={property.title}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
          {match && labelStyle && (
            <span
              className="card-a-match"
              style={{ background: labelStyle.bg, color: labelStyle.color, border: `1px solid ${labelStyle.border}` }}
            >
              {match.label}
            </span>
          )}
        </div>

        <div className="card-a-body">
          <h3 className="card-a-title">{property.title}</h3>
          <p className="card-a-location">
            {property.society_name} &middot; {property.area}
          </p>

          <div className="card-a-price-row">
            <span className="card-a-price">{formatPrice(property.price)}</span>
            <span className="card-a-ppsqft">{property.price_per_sqft.toLocaleString("en-IN")} /sqft</span>
          </div>

          <div className="card-a-specs">
            <span>{property.bhk} BHK</span>
            <span>&middot;</span>
            <span>{property.sqft} sqft</span>
            <span>&middot;</span>
            <span>{property.facing}</span>
            <span>&middot;</span>
            <span>Floor {property.floor}/{property.total_floors}</span>
          </div>

          {match && <p className="card-a-reason">{match.reason}</p>}

          {/* Structured match explanation — preference pills + reason badges */}
          {explanation && explanation.preference_coverage.length > 0 && (
            <MatchExplanationBlock explanation={explanation} />
          )}

          <div className="card-a-signals">
            {property.google_rating && (
              <span className="property-signal" style={{ display: "inline-flex", alignItems: "center", gap: "0.2rem" }}>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="#f59e0b" stroke="none">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                </svg>
                {property.google_rating.toFixed(1)}
                {property.google_review_count && (
                  <span style={{ color: "var(--color-text-muted)", fontSize: "0.72rem" }}>
                    ({property.google_review_count})
                  </span>
                )}
              </span>
            )}
            <span className="property-signal">
              {property.possession_status === "ready" ? "Ready to move" : "Under construction"}
            </span>
            <span className="property-signal">{property.metro_distance_mins} min to metro</span>
            <span className="property-signal">{property.builder_name}</span>
          </div>

          {property.transparency_tags.length > 0 && (
            <div className="card-a-tags">
              {property.transparency_tags.map((tag) => {
                const isDiscovery = tag === "Discovered via Search" || tag === "Verification Pending";
                return (
                  <span
                    key={tag}
                    className={isDiscovery ? "tag" : "tag tag-positive"}
                    style={isDiscovery ? {
                      background: tag === "Verification Pending" ? "#fff7ed" : "#f0fdf4",
                      color: tag === "Verification Pending" ? "#9a3412" : "#15803d",
                      border: `1px solid ${tag === "Verification Pending" ? "#fed7aa" : "#bbf7d0"}`,
                      fontSize: "0.72rem",
                    } : undefined}
                  >
                    {tag.replace(/_/g, " ")}
                  </span>
                );
              })}
            </div>
          )}
        </div>
      </Link>

      {/* Always-visible action bar */}
      <div className="card-a-actions">
        <button onClick={handleSave} className={`card-a-save-btn ${saved ? "card-a-save-btn--saved" : ""}`}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
          </svg>
          {saved ? "Subscribed" : "Subscribe"}
        </button>
        <button className="card-a-detail-btn" onClick={handleQuickView}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
          </svg>
          Quick view
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </button>
      </div>
    </div>
  );
}

/* ---------- Match Explanation Block ---------- */

function MatchExplanationBlock({ explanation }: { explanation: MatchExplanation }) {
  const [expanded, setExpanded] = useState(false);
  const allNoData = explanation.preference_coverage.every(pc => pc.status === "no_data");

  if (allNoData) {
    return (
      <p style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", margin: "0.35rem 0 0", lineHeight: 1.5 }}>
        Not enough data to evaluate your preferences for this property yet. Matched on location and specs.
      </p>
    );
  }

  const visibleReasons = expanded ? explanation.reasons : explanation.reasons.slice(0, 3);
  const hiddenCount = explanation.reasons.length - 3;

  return (
    <div style={{ marginTop: "0.35rem" }}>
      {/* Preference coverage pills */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: "0.3rem", marginBottom: "0.35rem" }}>
        {explanation.preference_coverage.map(pc => (
          <PreferencePill key={pc.preference} coverage={pc} />
        ))}
      </div>

      {/* Match reasons */}
      {visibleReasons.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: "0.25rem" }}>
          {visibleReasons.map(r => (
            <MatchReasonBadge key={`${r.fact_key}-${r.preference}`} reason={r} />
          ))}
          {!expanded && hiddenCount > 0 && (
            <button
              onClick={(e) => { e.preventDefault(); e.stopPropagation(); setExpanded(true); }}
              style={{
                background: "none",
                border: "none",
                color: "#3b5998",
                fontSize: "0.72rem",
                cursor: "pointer",
                padding: "0.15rem 0",
                textAlign: "left",
                fontFamily: "inherit",
              }}
            >
              +{hiddenCount} more {hiddenCount === 1 ? "reason" : "reasons"}
            </button>
          )}
        </div>
      )}

      {/* Graph vs legacy indicator */}
      {explanation.graph_driven_pct > 0 && (
        <p style={{ fontSize: "0.68rem", color: "var(--color-text-muted)", margin: "0.3rem 0 0" }}>
          {Math.round(explanation.graph_driven_pct)}% scored from verified data
        </p>
      )}
    </div>
  );
}

/* ---------- Results Page ---------- */

export function ResultsPageA() {
  const [properties, setProperties] = useState<PropertyCardType[]>([]);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  const [searchResponse, setSearchResponse] = useState<SearchResponse | null>(null);
  const [searchFailed, setSearchFailed] = useState(false);
  const [panelPropertyId, setPanelPropertyId] = useState<string | null>(null);
  const [compareIds, setCompareIds] = useState<string[]>(() => {
    try {
      const stored = sessionStorage.getItem("oe_compare_ids");
      return stored ? JSON.parse(stored) : [];
    } catch { return []; }
  });
  const [showCompare, setShowCompare] = useState(false);

  // Persist compare selections across navigation
  useEffect(() => {
    if (compareIds.length > 0) {
      sessionStorage.setItem("oe_compare_ids", JSON.stringify(compareIds));
    } else {
      sessionStorage.removeItem("oe_compare_ids");
    }
  }, [compareIds]);

  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const query = searchParams.get("q") || "";
  const areaFilter = searchParams.get("area") || "";
  const [searchInput, setSearchInput] = useState(query);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    const q = searchInput.trim();
    if (q) {
      sessionStorage.setItem("oe_search_query", q);
      setSearchParams({ q });
    } else {
      sessionStorage.removeItem("oe_search_query");
      // Preserve area filter if present
      if (areaFilter) {
        setSearchParams({ area: areaFilter });
      } else {
        setSearchParams({});
      }
    }
  };

  const clearAreaFilter = () => {
    if (query) {
      setSearchParams({ q: query });
    } else {
      setSearchParams({});
    }
  };

  // When there's a search query, call the backend search API.
  // When there's no query, load all properties.
  // No client-side fallback — if the backend is down, show an error.
  useEffect(() => {
    setSearchResponse(null);
    setSearchFailed(false);

    if (query) {
      searchProperties(query)
        .then((data) => {
          setSearchResponse(data);
          setStatus("ok");
        })
        .catch(() => {
          setSearchFailed(true);
          setStatus("error");
        });
    } else {
      getProperties()
        .then((data) => { setProperties(data); setStatus("ok"); })
        .catch(() => setStatus("error"));
    }
  }, [query]);

  const useBackendResults = query && searchResponse && !searchFailed;

  // Apply area filter client-side (from homepage area card clicks)
  const filtered = useMemo(() => {
    if (!areaFilter) return properties;
    const filter = areaFilter.toLowerCase();
    return properties.filter((p) => p.area.toLowerCase().includes(filter));
  }, [properties, areaFilter]);

  const matchResults: { property: PropertyCardType; match: MatchResult; explanation?: MatchExplanation }[] = useMemo(() => {
    if (useBackendResults) {
      return searchResponse.results.map((r) => ({
        property: r as PropertyCardType,
        match: {
          label: r.match_label as MatchResult["label"],
          reason: r.match_reason,
        },
        explanation: r.match_explanation,
      }));
    }
    // No query — show all properties without match labels
    return filtered.map((p) => ({ property: p, match: undefined as unknown as MatchResult }));
  }, [useBackendResults, searchResponse, filtered]);

  const areaContext: SearchAreaContext | null = useBackendResults ? searchResponse.area_context : null;
  const totalCount = useBackendResults ? searchResponse.total_results : filtered.length;
  const discoveryStatus = useBackendResults ? searchResponse.discovery_status : null;
  const discoveryCount = useBackendResults ? searchResponse.discovery_count : null;
  const intent = useBackendResults ? searchResponse.intent : null;

  if (status === "loading") return <PageState variant="loading" context="results" />;
  if (status === "error") {
    return (
      <div className="page-container">
        <div style={{ textAlign: "center", padding: "4rem 2rem" }}>
          <h2 style={{ fontSize: "1.3rem", fontWeight: 600, marginBottom: "0.75rem" }}>Could not load results</h2>
          <p style={{ color: "var(--color-text-secondary)", marginBottom: "1.5rem", lineHeight: 1.6 }}>
            The backend is unavailable. Please try again later.
          </p>
          <div style={{ display: "flex", gap: "0.75rem", justifyContent: "center" }}>
            <button className="btn btn-primary" onClick={() => window.location.reload()}>Retry</button>
            <button className="btn btn-outline" onClick={() => navigate("/")}>Return home</button>
          </div>
        </div>
      </div>
    );
  }

  const summary = intent
    ? formatSearchSummary({ query, area: intent.area ?? undefined, bhk: intent.bhk ?? undefined, budgetMax: intent.budget_max ?? undefined, preferences: intent.preferences })
    : null;
  const hasSearchChips = intent && (intent.area || intent.bhk || intent.budget_max || intent.preferences.length > 0);

  return (
    <div className="page-container">
      <div className="page-header">
        <h1>Properties</h1>

        {/* Inline search bar for refining */}
        <form
          onSubmit={handleSearch}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            margin: "0.75rem 0",
            padding: "0.65rem 1rem",
            borderRadius: "var(--radius-md)",
            backgroundColor: "var(--color-bg-card)",
            border: "1px solid var(--color-border)",
            maxWidth: "520px",
          }}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#999" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="Refine: area, BHK, budget, preferences..."
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            style={{
              flex: 1,
              border: "none",
              outline: "none",
              background: "transparent",
              fontSize: "0.88rem",
              fontFamily: "inherit",
              color: "var(--color-text)",
            }}
          />
          <button
            type="submit"
            style={{
              border: "none",
              background: "#1a1a1a",
              color: "#fff",
              padding: "0.4rem 1rem",
              borderRadius: "8px",
              fontSize: "0.82rem",
              cursor: "pointer",
              fontFamily: "inherit",
              whiteSpace: "nowrap",
            }}
          >
            Search
          </button>
        </form>

        {query && intent && (
          <div style={{ marginTop: "0.5rem" }}>
            {hasSearchChips && (
              <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", marginBottom: "0.5rem" }}>
                {intent.area && <span className="tag tag-neutral">{intent.area}</span>}
                {intent.bhk && <span className="tag tag-neutral">{intent.bhk} BHK</span>}
                {intent.budget_max && (
                  <span className="tag tag-neutral">
                    under {intent.budget_max >= 10_000_000 ? `${(intent.budget_max / 10_000_000).toFixed(1)} Cr` : `${(intent.budget_max / 100_000).toFixed(0)}L`}
                  </span>
                )}
                {intent.preferences.map((pref) => <span key={pref} className="tag tag-neutral">{pref}</span>)}
              </div>
            )}
            {summary && (
              <p style={{ color: "var(--color-text-muted)", fontSize: "0.8rem", margin: 0 }}>
                {summary}. Showing {totalCount} {totalCount === 1 ? "property" : "properties"}.
              </p>
            )}
          </div>
        )}

        {!query && (
          <div>
            {areaFilter && (
              <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", marginBottom: "0.5rem" }}>
                <span
                  className="tag tag-neutral"
                  style={{ display: "inline-flex", alignItems: "center", gap: "0.35rem", cursor: "pointer" }}
                  onClick={clearAreaFilter}
                >
                  {areaFilter}
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                    <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                </span>
              </div>
            )}
            <p>{totalCount} {areaFilter ? `listings in ${areaFilter}` : "listings with full transparency reports"}</p>
          </div>
        )}
      </div>

      {/* Discovery banner — shown when live discovery found new properties */}
      {discoveryStatus === "discovered_new" && discoveryCount && discoveryCount > 0 && (
        <div
          className="section-card"
          style={{
            padding: "0.85rem 1.25rem",
            marginBottom: "1.25rem",
            background: "linear-gradient(135deg, #f0fdf4 0%, #ecfdf5 100%)",
            border: "1px solid #bbf7d0",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#16a34a" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 2L2 7l10 5 10-5-10-5z" />
              <path d="M2 17l10 5 10-5" />
              <path d="M2 12l10 5 10-5" />
            </svg>
            <span style={{ fontSize: "0.85rem", fontWeight: 600, color: "#15803d" }}>
              Just discovered {discoveryCount} new {discoveryCount === 1 ? "property" : "properties"}
            </span>
          </div>
          <p style={{ margin: "0.35rem 0 0", fontSize: "0.78rem", color: "#166534", lineHeight: 1.5 }}>
            We searched the web in real time to find these. Details are being verified — look for the "Verification Pending" tag.
          </p>
        </div>
      )}

      {/* Area context bar — shown when backend search returns area info */}
      {areaContext && <AreaContextBar ctx={areaContext} />}

      {/* Knowledge graph insights removed — raw data not user-friendly yet */}

      <div
        className={panelPropertyId ? "results-grid--panel-open" : ""}
        style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))", gap: "1.25rem", paddingBottom: compareIds.length > 0 ? "100px" : 0, transition: "margin-right 0.3s var(--ease-out)" }}
      >
        {matchResults.map(({ property, match, explanation }) => (
          <CardA
            key={property.id}
            property={property}
            match={match}
            explanation={explanation}
            onQuickView={setPanelPropertyId}
            isComparing={compareIds.includes(property.id)}
            onToggleCompare={(id) => {
              setCompareIds(prev =>
                prev.includes(id)
                  ? prev.filter(x => x !== id)
                  : prev.length < MAX_COMPARE ? [...prev, id] : prev
              );
            }}
          />
        ))}
      </div>

      {/* Side panel — quick view */}
      {panelPropertyId && !showCompare && (() => {
        const panelCard = matchResults.find(r => r.property.id === panelPropertyId)?.property;
        if (!panelCard) return null;
        return (
          <PropertySidePanel
            propertyId={panelPropertyId}
            card={panelCard}
            onClose={() => setPanelPropertyId(null)}
            onAddCompare={(id) => {
              setCompareIds(prev =>
                prev.includes(id) ? prev : prev.length < MAX_COMPARE ? [...prev, id] : prev
              );
            }}
            isComparing={compareIds.includes(panelPropertyId)}
          />
        );
      })()}

      {/* Compare bar — floating tray */}
      <CompareBar
        items={compareIds.map(id => matchResults.find(r => r.property.id === id)?.property).filter(Boolean) as PropertyCardType[]}
        onRemove={(id) => setCompareIds(prev => prev.filter(x => x !== id))}
        onCompare={() => setShowCompare(true)}
        onClear={() => { setCompareIds([]); setShowCompare(false); }}
      />

      {/* Compare panel — side-by-side */}
      {showCompare && compareIds.length >= 2 && (
        <ComparePanel
          cards={compareIds.map(id => matchResults.find(r => r.property.id === id)?.property).filter(Boolean) as PropertyCardType[]}
          onClose={() => setShowCompare(false)}
          onRemove={(id) => {
            const next = compareIds.filter(x => x !== id);
            setCompareIds(next);
            if (next.length < 2) setShowCompare(false);
          }}
        />
      )}
    </div>
  );
}
