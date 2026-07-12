/**
 * Results page with inline decision-sheet actions and backend search integration.
 * In local development, the API layer can serve checked-in fixtures when the
 * Rust backend is unavailable so product review does not render a blank shell.
 */
import { useEffect, useState, useMemo } from "react";
import { useSearchParams, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { PropertyCard as PropertyCardType, SearchResponse, SearchAreaContext, MatchExplanation } from "../lib/types.ts";
import { getProperties, searchProperties } from "../lib/api.ts";
import { formatSearchSummary } from "../lib/search.ts";
import type { MatchResult } from "../lib/search.ts";
import { PageState } from "../components/PageState.tsx";
import { ImageWithFallback } from "../components/ImageWithFallback.tsx";
import { PreferencePill } from "../components/PreferencePill.tsx";
import { MatchReasonBadge } from "../components/MatchReasonBadge.tsx";
import { PropertySidePanel } from "../components/PropertySidePanel.tsx";
import { TrustBadge } from "../components/TrustBadge.tsx";
import { ProjectStatusTag } from "../components/ProjectStatusTag.tsx";
import { BuilderTrustBadge } from "../components/BuilderTrustBadge.tsx";
import { DataFreshnessBadge } from "../components/DataFreshnessBadge.tsx";
import { ConfidenceMeter } from "../components/ConfidenceMeter.tsx";
import { getShortlistedIds, isShortlisted, toggleShortlist } from "../lib/shortlist-store.ts";
import { addRecentSearch } from "../lib/recent-searches.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  return !!value && value.trim().length > 0 && value !== "Not specified";
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

function CardA({ property, match, explanation, confidenceScore, onQuickView, onSaveChange }: {
  property: PropertyCardType;
  match?: MatchResult;
  explanation?: MatchExplanation;
  confidenceScore?: import("../lib/types.ts").ConfidenceScore;
  onQuickView?: (id: string) => void;
  onSaveChange?: () => void;
}) {
  const [saved, setSaved] = useState(() => isShortlisted(property.id));
  const specs = [
    `${property.bhk} BHK`,
    hasKnownNumber(property.sqft) ? `${property.sqft.toLocaleString("en-IN")} sqft` : null,
    isKnownText(property.facing) ? property.facing : null,
    hasKnownNumber(property.floor) && hasKnownNumber(property.total_floors)
      ? `Floor ${property.floor}/${property.total_floors}`
      : null,
  ].filter((spec): spec is string => spec !== null);

  const handleSave = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setSaved(toggleShortlist(property.id));
    onSaveChange?.();
  };

  const handleQuickView = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onQuickView?.(property.id);
  };

  const labelStyle = match ? LABEL_COLORS[match.label] || LABEL_COLORS["Good match"] : null;

  return (
    <div className="card-a">
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
            {property.society_name ? `${property.society_name} · ` : ""}{property.area}
          </p>

          <div className="card-a-price-row">
            <span className="card-a-price">{formatPrice(property.price)}</span>
            {hasKnownNumber(property.price_per_sqft) && (
              <span className="card-a-ppsqft">{property.price_per_sqft.toLocaleString("en-IN")} /sqft</span>
            )}
          </div>

          <div className="card-a-specs">
            {specs.map((spec, index) => (
              <span key={spec}>
                {index > 0 && <span>&middot; </span>}
                {spec}
              </span>
            ))}
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
            <ProjectStatusTag
              status={property.project_status}
              displayText={property.project_status_display}
              possessionStatus={property.possession_status}
            />
            {hasKnownNumber(property.metro_distance_mins) && (
              <span className="property-signal">{property.metro_distance_mins} min to metro</span>
            )}
            {isKnownText(property.builder_name) && (
              <span className="property-signal">{property.builder_name}</span>
            )}
            <BuilderTrustBadge deliveryDisplay={property.builder_delivery_display} compact />
            <TrustBadge rootSource={property.root_source} compact />
            <DataFreshnessBadge freshness={property.data_freshness} compact />
            <ConfidenceMeter confidence={confidenceScore} compact />
          </div>

          {property.transparency_tags.length > 0 && (
            <div className="card-a-tags">
              {property.transparency_tags.map((tag) => {
                const isDiscovery = tag === "Discovered via Search" || tag === "Verification Pending";
                const isSellerRegistered = tag === "seller-registered";
                const isVerificationPending = tag === "verification-pending";
                const isSpecial = isDiscovery || isSellerRegistered || isVerificationPending;
                const tagStyle = isSellerRegistered
                  ? {
                      background: "#fffbeb",
                      color: "#92400e",
                      border: "1px solid #fcd34d",
                      fontSize: "0.72rem",
                    }
                  : isVerificationPending
                  ? {
                      background: "#f9fafb",
                      color: "#6b7280",
                      border: "1px solid #e5e7eb",
                      fontSize: "0.72rem",
                    }
                  : isDiscovery ? {
                      background: tag === "Verification Pending" ? "#fff7ed" : "#f0fdf4",
                      color: tag === "Verification Pending" ? "#9a3412" : "#15803d",
                      border: `1px solid ${tag === "Verification Pending" ? "#fed7aa" : "#bbf7d0"}`,
                      fontSize: "0.72rem",
                    } : undefined;
                return (
                  <span
                    key={tag}
                    className={isSpecial ? "tag" : "tag tag-positive"}
                    style={tagStyle}
                  >
                    {tag.replace(/_/g, " ").replace(/-/g, " ")}
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
          {saved ? "In decision sheet" : "Save to sheet"}
        </button>
        <button className="card-a-detail-btn" onClick={handleQuickView}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" />
          </svg>
          <span className="card-a-detail-btn-label">Quick view</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </button>
      </div>
    </div>
  );
}

function DecisionSheetDock({ count }: { count: number }) {
  if (count === 0) return null;

  return (
    <div className="decision-dock">
      <div className="decision-dock-copy">
        <span className="decision-dock-kicker">Decision sheet</span>
        <strong>{count} saved {count === 1 ? "candidate" : "candidates"}</strong>
      </div>
      <Link to="/shortlist" className="decision-dock-link">
        Open workspace
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
        </svg>
      </Link>
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
  const [savedCount, setSavedCount] = useState(() => getShortlistedIds().length);
  const refreshSavedCount = () => setSavedCount(getShortlistedIds().length);

  const [searchParams, setSearchParams] = useSearchParams();
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
  // The API layer owns the development fixture fallback when the backend is down.
  useEffect(() => {
    let cancelled = false;

    queueMicrotask(() => {
      if (cancelled) return;
      setSearchResponse(null);
      setSearchFailed(false);
    });

    if (query) {
      addRecentSearch(query);
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
  }, [query]);

  const useBackendResults = query && searchResponse && !searchFailed;

  // Apply area filter client-side (from homepage area card clicks)
  const filtered = useMemo(() => {
    if (!areaFilter) return properties;
    const filter = areaFilter.toLowerCase();
    return properties.filter((p) => p.area.toLowerCase().includes(filter));
  }, [properties, areaFilter]);

  const matchResults: { property: PropertyCardType; match: MatchResult; explanation?: MatchExplanation; confidenceScore?: import("../lib/types.ts").ConfidenceScore }[] = useMemo(() => {
    if (useBackendResults) {
      return searchResponse.results.map((r) => ({
        property: r as PropertyCardType,
        match: {
          label: r.match_label as MatchResult["label"],
          reason: r.match_reason,
        },
        explanation: r.match_explanation,
        confidenceScore: r.confidence_score,
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

  if (status === "loading") return (
    <div className="page-container">
      <div className="page-header">
        <h1>Properties</h1>
        <div className="skeleton-search-bar skeleton-bar" />
      </div>
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
      <div className="page-container">
        <PageState variant="error" context="results" />
      </div>
    );
  }

  const hardConstraints = intent?.hard_constraints ?? [];
  const hardConstraintLabels = hardConstraints.map((constraint) => constraint.raw_text);
  const summary = intent
    ? formatSearchSummary({
        query,
        area: intent.area ?? undefined,
        bhk: intent.bhk ?? undefined,
        budgetMax: intent.budget_max ?? undefined,
        hardConstraints,
        preferences: intent.preferences,
      })
    : null;
  const hasSearchChips = intent && (intent.area || intent.bhk || intent.budget_max || hardConstraints.length > 0 || intent.preferences.length > 0);

  const helmetTitle = query
    ? `${query} — Property Search | OpenEstates`
    : "All Properties — OpenEstates";
  const helmetDescription = query
    ? `${totalCount} ${totalCount === 1 ? "property" : "properties"} matching "${query}"${intent?.area ? ` in ${intent.area}` : ""}${hardConstraintLabels.length ? `. Constraints: ${hardConstraintLabels.join(", ")}` : ""}${intent?.preferences?.length ? `. Preferences: ${intent.preferences.join(", ")}` : ""}.`
    : `Browse ${totalCount} properties with full transparency reports on OpenEstates.`;

  return (
    <div className="page-container">
      <Helmet>
        <title>{helmetTitle}</title>
        <meta name="description" content={helmetDescription} />
        <meta property="og:title" content={helmetTitle} />
        <meta property="og:description" content={helmetDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      <div className="page-header">
        <h1>Properties</h1>

        {/* Inline search bar for refining */}
        <form
          onSubmit={handleSearch}
          className="results-search-bar"
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
                {hardConstraintLabels.map((label) => <span key={label} className="tag tag-neutral">{label}</span>)}
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

      {/* Accessible live region — announces result count to screen readers */}
      <div aria-live="polite" className="sr-only">
        {query
          ? `${totalCount} ${totalCount === 1 ? "property" : "properties"} found for "${query}".`
          : `Showing ${totalCount} ${totalCount === 1 ? "property" : "properties"}.`}
      </div>

      {/* Deprecated compatibility banner; backend no longer performs request-time discovery. */}
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

      <DecisionSheetDock count={savedCount} />

      {/* Knowledge graph insights removed — raw data not user-friendly yet */}

      {matchResults.length === 0 && query && (
        <div className="empty-state">
          <h2>No properties match "{query}"</h2>
          <p>Try broadening your search or explore one of these suggestions.</p>
          <div className="empty-state-chips">
            {intent?.area && (
              <button className="empty-state-chip" onClick={() => setSearchParams({ q: intent.area! })}>
                Just {intent.area}
              </button>
            )}
            {intent?.bhk && (
              <button className="empty-state-chip" onClick={() => {
                const without = query.replace(/\d+\s*bhk/i, "").trim();
                if (without) setSearchParams({ q: without });
              }}>
                Without BHK filter
              </button>
            )}
            {["3BHK Whitefield under 2Cr", "Family-friendly Sarjapur", "Near metro Bellandur"].map((s) => (
              <button key={s} className="empty-state-chip" onClick={() => setSearchParams({ q: s })}>
                {s}
              </button>
            ))}
          </div>
          <Link to="/results" style={{ color: "var(--color-accent)", fontSize: "0.88rem", fontWeight: 500 }}>
            Browse all properties
          </Link>
        </div>
      )}

      <div
        className={`results-grid ${panelPropertyId ? "results-grid--panel-open" : ""}`}
        style={{ transition: "margin-right 0.3s var(--ease-out)" }}
      >
        {matchResults.map(({ property, match, explanation, confidenceScore }) => (
          <CardA
            key={property.id}
            property={property}
            match={match}
            explanation={explanation}
            confidenceScore={confidenceScore}
            onQuickView={setPanelPropertyId}
            onSaveChange={refreshSavedCount}
          />
        ))}
      </div>

      {/* Side panel — quick view */}
      {panelPropertyId && (() => {
        const panelCard = matchResults.find(r => r.property.id === panelPropertyId)?.property;
        if (!panelCard) return null;
        return (
          <PropertySidePanel
            propertyId={panelPropertyId}
            card={panelCard}
            onClose={() => setPanelPropertyId(null)}
            onSaveChange={refreshSavedCount}
          />
        );
      })()}
    </div>
  );
}
