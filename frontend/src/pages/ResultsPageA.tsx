/**
 * Results page with inline save actions and backend search integration.
 * In local development, the API layer can serve checked-in fixtures when the
 * Rust backend is unavailable so product review does not render a blank shell.
 */
import { useEffect, useState, useMemo, useCallback, type FormEvent, type ReactNode } from "react";
import { useSearchParams, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { ConfidenceScore, PropertyCard as PropertyCardType, SearchResponse, SearchAreaContext, MatchExplanation } from "../lib/types.ts";
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
import {
  getSheetItems,
  isOnSheet,
  removeFromSheet,
  SHEET_UPDATED_EVENT,
  toggleSheetItem,
  type SheetItem,
} from "../lib/sheet-store.ts";
import { addRecentSearch } from "../lib/recent-searches.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function formatMetric(value: number | null | undefined, suffix = ""): string {
  if (!hasKnownNumber(value)) return "—";
  return `${value.toLocaleString("en-IN")}${suffix}`;
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
  const [onSheet, setOnSheet] = useState(() => isOnSheet(property.id));
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
    setOnSheet(toggleSheetItem(property.id));
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
        <button onClick={handleSave} className={`card-a-save-btn ${onSheet ? "card-a-save-btn--saved" : ""}`}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill={onSheet ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
          </svg>
          {onSheet ? "Saved" : "Save"}
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

function rootSourceLabel(source: string | undefined): string {
  if (source === "rera") return "RERA";
  if (source === "seller") return "Seller source";
  if (source === "discovery") return "Discovered";
  return "Source pending";
}

function sheetSignals(property: PropertyCardType): string[] {
  const signals = [
    rootSourceLabel(property.root_source),
    property.google_rating ? `Google ${property.google_rating.toFixed(1)}` : null,
    property.project_status_display,
    property.data_freshness?.fact_count ? `${property.data_freshness.fact_count} facts` : null,
  ].filter((signal): signal is string => !!signal && signal.trim().length > 0);

  return signals.slice(0, 4);
}

function SheetTray({
  items,
  propertiesById,
  onRemove,
}: {
  items: SheetItem[];
  propertiesById: Map<string, PropertyCardType>;
  onRemove: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  if (items.length === 0) return null;

  const visibleItems = items.slice(0, 6);

  return (
    <div className={`sheet-tray${open ? " sheet-tray--open" : ""}`}>
      <button type="button" className="sheet-tray-toggle" onClick={() => setOpen((value) => !value)}>
        <span className="sheet-tray-copy">
          <span className="sheet-tray-kicker">Saved</span>
          <strong>{items.length} saved {items.length === 1 ? "home" : "homes"}</strong>
        </span>
        <span className="sheet-tray-action">
          {open ? "Close" : "Open"}
          <i aria-hidden="true" />
        </span>
      </button>

      {open && (
        <div className="sheet-tray-panel">
          {visibleItems.map((item) => {
            const property = propertiesById.get(item.id);
            const signals = property ? sheetSignals(property) : [];
            return (
              <div key={item.id} className="sheet-tray-item">
                {property ? (
                  <Link to={`/property/${property.id}`} className="sheet-tray-link">
                    <span className="sheet-tray-image">
                      <ImageWithFallback src={property.hero_image} alt={property.title} style={{ width: "100%", height: "100%" }} />
                    </span>
                    <span className="sheet-tray-item-copy">
                      <strong>{property.title}</strong>
                      <span>{property.area} · {formatPrice(property.price)}</span>
                      <span className="sheet-tray-meta">
                        {signals.map((signal) => (
                          <em key={signal}>{signal}</em>
                        ))}
                      </span>
                    </span>
                  </Link>
                ) : (
                  <span className="sheet-tray-link sheet-tray-link--missing">
                    <span className="sheet-tray-image" />
                    <span className="sheet-tray-item-copy">
                      <strong>Saved home</strong>
                      <span>Refresh results to reload this home.</span>
                    </span>
                  </span>
                )}
                <button type="button" className="sheet-tray-remove" onClick={() => onRemove(item.id)} aria-label="Remove from sheet">
                  -
                </button>
              </div>
            );
          })}

          {items.length > visibleItems.length && (
            <p className="sheet-tray-overflow">
              {items.length - visibleItems.length} more saved. Refine search to bring them back into view.
            </p>
          )}
        </div>
      )}
    </div>
  );
}

type ResultsViewMode = "cards" | "sheet";
type SheetSortKey = "sheet" | "price" | "carpet" | "efficiency" | "carpetCost";

type SheetCompareRow = {
  property: PropertyCardType;
  item?: SheetItem;
  onSheet: boolean;
};

function ViewModeSwitch({
  mode,
  sheetCount,
  onChange,
}: {
  mode: ResultsViewMode;
  sheetCount: number;
  onChange: (mode: ResultsViewMode) => void;
}) {
  return (
    <div className="results-view-switch" aria-label="Results view">
      <button
        type="button"
        className={mode === "cards" ? "results-view-switch-btn results-view-switch-btn--active" : "results-view-switch-btn"}
        onClick={() => onChange("cards")}
        aria-pressed={mode === "cards"}
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <rect x="3" y="3" width="7" height="7" rx="1.5" />
          <rect x="14" y="3" width="7" height="7" rx="1.5" />
          <rect x="3" y="14" width="7" height="7" rx="1.5" />
          <rect x="14" y="14" width="7" height="7" rx="1.5" />
        </svg>
        Results
      </button>
      <button
        type="button"
        className={mode === "sheet" ? "results-view-switch-btn results-view-switch-btn--active" : "results-view-switch-btn"}
        onClick={() => onChange("sheet")}
        aria-pressed={mode === "sheet"}
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M4 6h16" />
          <path d="M4 12h16" />
          <path d="M4 18h16" />
          <path d="M9 4v16" />
          <path d="M15 4v16" />
        </svg>
        Saved
        {sheetCount > 0 && <span>{sheetCount}</span>}
      </button>
    </div>
  );
}

function knownMetric(value: number | null | undefined): number | null {
  return hasKnownNumber(value) ? value : null;
}

function carpetSqft(property: PropertyCardType): number | null {
  return knownMetric(property.carpet_area_sqft ?? property.sqft);
}

function totalSqft(property: PropertyCardType): number | null {
  return knownMetric(property.super_builtup_sqft);
}

function carpetEfficiency(property: PropertyCardType): number | null {
  const carpet = carpetSqft(property);
  const total = totalSqft(property);
  if (!carpet || !total || carpet > total) return null;
  return carpet / total;
}

function carpetCost(property: PropertyCardType): number | null {
  const carpet = carpetSqft(property);
  if (!carpet || !hasKnownNumber(property.price)) return null;
  return Math.round(property.price / carpet);
}

function formatPercent(value: number | null): string {
  if (!value || !Number.isFinite(value)) return "—";
  return `${Math.round(value * 100)}%`;
}

function sortValue(row: SheetCompareRow, key: SheetSortKey): number {
  if (key === "price") return row.property.price || Number.MAX_SAFE_INTEGER;
  if (key === "carpet") {
    const value = carpetSqft(row.property);
    return value ? -value : Number.MAX_SAFE_INTEGER;
  }
  if (key === "efficiency") {
    const value = carpetEfficiency(row.property);
    return value ? -value : Number.MAX_SAFE_INTEGER;
  }
  if (key === "carpetCost") return carpetCost(row.property) ?? Number.MAX_SAFE_INTEGER;
  return row.onSheet ? 0 : 1;
}

function SheetSortButton({
  sortKey,
  active,
  onSort,
  children,
}: {
  sortKey: SheetSortKey;
  active: boolean;
  onSort: (key: SheetSortKey) => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={active ? "comparison-sort-btn comparison-sort-btn--active" : "comparison-sort-btn"}
      onClick={() => onSort(sortKey)}
    >
      {children}
    </button>
  );
}

function SheetCompareView({
  rows,
  missingSheetCount,
  sortKey,
  onSortChange,
  onQuickView,
  onSheetToggle,
}: {
  rows: SheetCompareRow[];
  missingSheetCount: number;
  sortKey: SheetSortKey;
  onSortChange: (key: SheetSortKey) => void;
  onQuickView: (id: string) => void;
  onSheetToggle: (id: string) => void;
}) {
  const sortedRows = useMemo(() => {
    return rows
      .map((row, index) => ({ row, index }))
      .sort((a, b) => {
        const diff = sortValue(a.row, sortKey) - sortValue(b.row, sortKey);
        return diff || a.index - b.index;
      })
      .map(({ row }) => row);
  }, [rows, sortKey]);

  if (rows.length === 0) {
    return (
      <section className="comparison-sheet comparison-sheet--empty">
        <div>
          <h2>No saved homes yet</h2>
        </div>
        <p>Save homes from discovery to compare price, area, and usable space in one place.</p>
      </section>
    );
  }

  return (
    <section className="comparison-sheet" aria-label="Saved homes">
      <div className="comparison-sheet-toolbar">
        <div className="comparison-sort-group" aria-label="Sort saved homes">
          <SheetSortButton sortKey="sheet" active={sortKey === "sheet"} onSort={onSortChange}>Added</SheetSortButton>
          <SheetSortButton sortKey="price" active={sortKey === "price"} onSort={onSortChange}>Price</SheetSortButton>
          <SheetSortButton sortKey="carpet" active={sortKey === "carpet"} onSort={onSortChange}>Carpet</SheetSortButton>
          <SheetSortButton sortKey="efficiency" active={sortKey === "efficiency"} onSort={onSortChange}>Eff.</SheetSortButton>
          <SheetSortButton sortKey="carpetCost" active={sortKey === "carpetCost"} onSort={onSortChange}>₹/carpet</SheetSortButton>
        </div>
      </div>

      <div className="comparison-table-scroll">
        <table className="comparison-table">
          <thead>
            <tr>
              <th className="comparison-table-home">Home</th>
              <th>Area</th>
              <th>BHK</th>
              <th>Price</th>
              <th>Carpet</th>
              <th>Total</th>
              <th>Eff.</th>
              <th>₹/carpet</th>
              <th>Status</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {sortedRows.map(({ property, onSheet }) => (
              <tr key={property.id}>
                <td className="comparison-table-home">
                  <Link to={`/property/${property.id}`} className="comparison-home-link">
                    <strong>{property.title}</strong>
                    <span>{isKnownText(property.builder_name) ? property.builder_name : "Builder pending"}</span>
                  </Link>
                </td>
                <td>{property.area || "—"}</td>
                <td>{property.bhk ? `${property.bhk}` : "—"}</td>
                <td className="comparison-num">{property.price ? formatPrice(property.price) : "—"}</td>
                <td className="comparison-num">{formatMetric(carpetSqft(property))}</td>
                <td className="comparison-num">{formatMetric(totalSqft(property))}</td>
                <td className="comparison-num">{formatPercent(carpetEfficiency(property))}</td>
                <td className="comparison-num">{formatMetric(carpetCost(property))}</td>
                <td>
                  <ProjectStatusTag
                    status={property.project_status}
                    possessionStatus={property.possession_status}
                  />
                </td>
                <td className="comparison-actions">
                  <button type="button" className="comparison-icon-btn" onClick={() => onQuickView(property.id)} aria-label={`Quick view ${property.title}`}>
                    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <circle cx="12" cy="12" r="10" />
                      <path d="M12 16v-4" />
                      <path d="M12 8h.01" />
                    </svg>
                  </button>
                  <button
                    type="button"
                    className={onSheet ? "comparison-sheet-toggle comparison-sheet-toggle--on" : "comparison-sheet-toggle"}
                    onClick={() => onSheetToggle(property.id)}
                  >
                    {onSheet ? "Saved" : "Save"}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {missingSheetCount > 0 && (
        <p className="comparison-sheet-footnote">
          {missingSheetCount} saved {missingSheetCount === 1 ? "home is" : "homes are"} outside the loaded catalog.
        </p>
      )}
    </section>
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

/* ---------- Search Experience ---------- */

type SearchExperienceProps = {
  variant?: "page" | "embedded";
  onSearchCommit?: (query: string) => void;
};

export function SearchExperience({ variant = "page", onSearchCommit }: SearchExperienceProps) {
  const isEmbedded = variant === "embedded";
  const [properties, setProperties] = useState<PropertyCardType[]>([]);
  const [catalogProperties, setCatalogProperties] = useState<PropertyCardType[]>([]);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  const [searchResponse, setSearchResponse] = useState<SearchResponse | null>(null);
  const [searchFailed, setSearchFailed] = useState(false);
  const [panelPropertyId, setPanelPropertyId] = useState<string | null>(null);
  const [sheetItems, setSheetItems] = useState<SheetItem[]>(() => getSheetItems());
  const [sheetSortKey, setSheetSortKey] = useState<SheetSortKey>("sheet");
  const refreshSheetItems = useCallback(() => setSheetItems(getSheetItems()), []);

  const [searchParams, setSearchParams] = useSearchParams();
  const query = searchParams.get("q") || "";
  const areaFilter = searchParams.get("area") || "";
  const viewMode: ResultsViewMode = searchParams.get("view") === "sheet" ? "sheet" : "cards";
  const [searchInput, setSearchInput] = useState(query);

  useEffect(() => {
    setSearchInput(query);
  }, [query]);

  useEffect(() => {
    window.addEventListener("storage", refreshSheetItems);
    window.addEventListener(SHEET_UPDATED_EVENT, refreshSheetItems);
    return () => {
      window.removeEventListener("storage", refreshSheetItems);
      window.removeEventListener(SHEET_UPDATED_EVENT, refreshSheetItems);
    };
  }, [refreshSheetItems]);

  const handleSearch = (e: FormEvent) => {
    e.preventDefault();
    const q = searchInput.trim();
    const nextParams = new URLSearchParams();
    if (viewMode === "sheet") nextParams.set("view", "sheet");
    if (q) {
      sessionStorage.setItem("oe_search_query", q);
      onSearchCommit?.(q);
      nextParams.set("q", q);
      setSearchParams(nextParams);
    } else {
      sessionStorage.removeItem("oe_search_query");
      // Preserve area filter if present
      if (areaFilter) nextParams.set("area", areaFilter);
      setSearchParams(nextParams);
    }
  };

  const clearAreaFilter = () => {
    const nextParams = new URLSearchParams();
    if (viewMode === "sheet") nextParams.set("view", "sheet");
    if (query) nextParams.set("q", query);
    setSearchParams(nextParams);
  };

  const setViewMode = (mode: ResultsViewMode) => {
    const nextParams = new URLSearchParams(searchParams);
    if (mode === "sheet") {
      nextParams.set("view", "sheet");
    } else {
      nextParams.delete("view");
    }
    setSearchParams(nextParams);
  };

  const setQueryPreservingView = (nextQuery: string) => {
    const nextParams = new URLSearchParams();
    if (viewMode === "sheet") nextParams.set("view", "sheet");
    if (nextQuery) nextParams.set("q", nextQuery);
    setSearchParams(nextParams);
  };

  useEffect(() => {
    let cancelled = false;
    getProperties()
      .then((data) => {
        if (!cancelled) setCatalogProperties(data);
      })
      .catch(() => {
        if (!cancelled) setCatalogProperties([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

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
  }, [query, onSearchCommit]);

  const useBackendResults = query && searchResponse && !searchFailed;

  // Apply area filter client-side (from homepage area card clicks)
  const filtered = useMemo(() => {
    if (!areaFilter) return properties;
    const filter = areaFilter.toLowerCase();
    return properties.filter((p) => p.area.toLowerCase().includes(filter));
  }, [properties, areaFilter]);

  const matchResults: { property: PropertyCardType; match?: MatchResult; explanation?: MatchExplanation; confidenceScore?: ConfidenceScore }[] = useMemo(() => {
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
    return filtered.map((p) => ({ property: p }));
  }, [useBackendResults, searchResponse, filtered]);

  const propertiesById = useMemo(() => {
    const next = new Map<string, PropertyCardType>();
    for (const property of catalogProperties) next.set(property.id, property);
    for (const property of properties) next.set(property.id, property);
    for (const { property } of matchResults) next.set(property.id, property);
    return next;
  }, [catalogProperties, matchResults, properties]);

  const removeSheetItem = (id: string) => {
    removeFromSheet(id);
    refreshSheetItems();
  };

  const toggleSheetFromComparison = (id: string) => {
    toggleSheetItem(id);
    refreshSheetItems();
  };

  const sheetCompareRows = useMemo<SheetCompareRow[]>(() => {
    const savedRows: SheetCompareRow[] = [];
    for (const item of sheetItems) {
      const property = propertiesById.get(item.id);
      if (!property) continue;
      savedRows.push({
        property,
        item,
        onSheet: true,
      });
    }
    return savedRows;
  }, [propertiesById, sheetItems]);

  const missingSheetCount = useMemo(() => {
    return sheetItems.filter((item) => !propertiesById.has(item.id)).length;
  }, [propertiesById, sheetItems]);
  const savedCount = sheetCompareRows.length;
  const savedCountLabel = `${savedCount} ${savedCount === 1 ? "saved home" : "saved homes"}`;

  const areaContext: SearchAreaContext | null = useBackendResults ? searchResponse.area_context : null;
  const totalCount = useBackendResults ? searchResponse.total_results : filtered.length;
  const discoveryStatus = useBackendResults ? searchResponse.discovery_status : null;
  const discoveryCount = useBackendResults ? searchResponse.discovery_count : null;
  const intent = useBackendResults ? searchResponse.intent : null;
  const containerClass = isEmbedded ? "inline-results-shell" : viewMode === "sheet" ? "page-container-wide" : "page-container";
  const headerClass = isEmbedded ? "inline-results-header" : "page-header";
  const showEmbeddedSwitch = isEmbedded && (Boolean(query) || viewMode === "sheet");
  const kicker = viewMode === "sheet" && !query ? "Saved homes" : "Search results";
  const title = !isEmbedded && viewMode === "sheet" ? "Saved" : "Properties";

  if (status === "loading") return (
    <div className={containerClass}>
      <div className={headerClass}>
        {showEmbeddedSwitch && <span className="inline-results-kicker">{kicker}</span>}
        {showEmbeddedSwitch ? (
          <ViewModeSwitch mode={viewMode} sheetCount={sheetItems.length} onChange={setViewMode} />
        ) : (
          <h1>{title}</h1>
        )}
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
      <div className={containerClass}>
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
    <div className={containerClass}>
      <Helmet>
        <title>{helmetTitle}</title>
        <meta name="description" content={helmetDescription} />
        <meta property="og:title" content={helmetTitle} />
        <meta property="og:description" content={helmetDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      <div className={headerClass}>
        {showEmbeddedSwitch && <span className="inline-results-kicker">{kicker}</span>}
        {showEmbeddedSwitch ? (
          <ViewModeSwitch mode={viewMode} sheetCount={sheetItems.length} onChange={setViewMode} />
        ) : (
          <h1>{title}</h1>
        )}

        {/* Inline search bar for refining */}
        <form
          onSubmit={handleSearch}
          className={`results-search-bar ${isEmbedded ? "results-search-bar--embedded" : ""}`}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#999" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            className="results-search-input"
            type="text"
            placeholder="Refine: area, BHK, budget, preferences..."
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
          />
          <button
            className="results-search-submit"
            type="submit"
          >
            Search
          </button>
        </form>

        {!isEmbedded && query && intent && (
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
            {viewMode !== "sheet" && areaFilter && (
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
            <p>
              {viewMode === "sheet"
                ? savedCount > 0
                  ? `${savedCountLabel} ready to compare.`
                  : "Your saved homes will stay here while you keep browsing."
                : `${totalCount} ${areaFilter ? `listings in ${areaFilter}` : "listings with full transparency reports"}`}
            </p>
          </div>
        )}
      </div>

      {/* Accessible live region — announces result count to screen readers */}
      <div aria-live="polite" className="sr-only">
        {viewMode === "sheet" && !query
          ? savedCount > 0
            ? `${savedCountLabel} ready to compare.`
            : "No saved homes yet."
          : query
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
      {!isEmbedded && areaContext && <AreaContextBar ctx={areaContext} />}

      {!isEmbedded && (
        <div className="results-view-row">
          <ViewModeSwitch mode={viewMode} sheetCount={sheetItems.length} onChange={setViewMode} />
        </div>
      )}

      {viewMode === "cards" && (
        <SheetTray items={sheetItems} propertiesById={propertiesById} onRemove={removeSheetItem} />
      )}

      {/* Knowledge graph insights removed — raw data not user-friendly yet */}

      {matchResults.length === 0 && query && (
        <div className="empty-state">
          <h2>No properties match "{query}"</h2>
          <p>Try broadening your search or explore one of these suggestions.</p>
          <div className="empty-state-chips">
            {intent?.area && (
              <button className="empty-state-chip" onClick={() => setQueryPreservingView(intent.area!)}>
                Just {intent.area}
              </button>
            )}
            {intent?.bhk && (
              <button className="empty-state-chip" onClick={() => {
                const without = query.replace(/\d+\s*bhk/i, "").trim();
                if (without) setQueryPreservingView(without);
              }}>
                Without BHK filter
              </button>
            )}
            {["3BHK Whitefield under 2Cr", "Family-friendly Sarjapur", "Near metro Bellandur"].map((s) => (
              <button key={s} className="empty-state-chip" onClick={() => setQueryPreservingView(s)}>
                {s}
              </button>
            ))}
          </div>
          {isEmbedded ? (
            <button
              type="button"
              className="inline-results-clear"
              onClick={() => setSearchParams({})}
            >
              Browse all properties
            </button>
          ) : (
            <Link to="/results" style={{ color: "var(--color-accent)", fontSize: "0.88rem", fontWeight: 500 }}>
              Browse all properties
            </Link>
          )}
        </div>
      )}

      {viewMode === "sheet" ? (
        <SheetCompareView
          rows={sheetCompareRows}
          missingSheetCount={missingSheetCount}
          sortKey={sheetSortKey}
          onSortChange={setSheetSortKey}
          onQuickView={setPanelPropertyId}
          onSheetToggle={toggleSheetFromComparison}
        />
      ) : (
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
                onSaveChange={refreshSheetItems}
              />
          ))}
        </div>
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
            onSaveChange={refreshSheetItems}
          />
        );
      })()}
    </div>
  );
}

/* ---------- Results Page ---------- */

export function ResultsPageA() {
  return <SearchExperience variant="page" />;
}
