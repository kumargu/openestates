/**
 * DataFreshnessBadge -- compact pill showing how recently data was updated.
 * Green for fresh (<7d), blue for recent (<30d), amber for stale (>30d).
 * Renders nothing when data_freshness is undefined.
 */

import type { DataFreshness } from "../lib/types.ts";

type DataFreshnessBadgeProps = {
  freshness?: DataFreshness;
  compact?: boolean;
};

function getFreshnessConfig(daysAgo: number): { bg: string; color: string; border: string; label: string } {
  if (daysAgo < 7) {
    return {
      bg: "#f0fdf4",
      color: "#15803d",
      border: "#bbf7d0",
      label: `Updated ${daysAgo}d ago`,
    };
  }
  if (daysAgo < 30) {
    return {
      bg: "#eff6ff",
      color: "#1d4ed8",
      border: "#bfdbfe",
      label: `Updated ${daysAgo}d ago`,
    };
  }
  return {
    bg: "#fffbeb",
    color: "#92400e",
    border: "#fcd34d",
    label: `Updated ${daysAgo}d ago`,
  };
}

export function DataFreshnessBadge({ freshness, compact = false }: DataFreshnessBadgeProps) {
  if (!freshness) return null;

  const config = getFreshnessConfig(freshness.days_ago);
  const fontSize = compact ? "0.68rem" : "0.75rem";
  const padding = compact ? "0.1rem 0.4rem" : "0.2rem 0.55rem";

  return (
    <span
      title={`${freshness.fact_count} facts from ${Object.entries(freshness.source_breakdown).map(([k, v]) => `${k}: ${v}`).join(", ")}`}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.25rem",
        fontSize,
        fontWeight: 600,
        padding,
        borderRadius: "999px",
        backgroundColor: config.bg,
        color: config.color,
        border: `1px solid ${config.border}`,
        whiteSpace: "nowrap",
        lineHeight: 1.4,
      }}
    >
      <svg width={compact ? 10 : 12} height={compact ? 10 : 12} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="10" />
        <polyline points="12 6 12 12 16 14" />
      </svg>
      {config.label}
    </span>
  );
}
