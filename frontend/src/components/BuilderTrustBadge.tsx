/**
 * BuilderTrustBadge — compact pill showing builder delivery track record.
 * Green for 90-100%, amber for 60-89%, red for below 60%.
 * Renders nothing when no delivery data is available.
 */

type BuilderTrustBadgeProps = {
  deliveryDisplay?: string;
  deliveryRate?: number;
  compact?: boolean;
};

export function BuilderTrustBadge({ deliveryDisplay, deliveryRate, compact = false }: BuilderTrustBadgeProps) {
  // Nothing to show if no delivery data
  if (!deliveryDisplay && deliveryRate == null) return null;

  // Determine color tier
  const rate = deliveryRate ?? 1.0;
  let bg: string;
  let color: string;
  let border: string;

  if (rate >= 0.9) {
    bg = "#f0fdf4";
    color = "#15803d";
    border = "#bbf7d0";
  } else if (rate >= 0.6) {
    bg = "#fffbeb";
    color = "#92400e";
    border = "#fcd34d";
  } else {
    bg = "#fef2f2";
    color = "#991b1b";
    border = "#fecaca";
  }

  const text = deliveryDisplay || `Builder: ${Math.round(rate * 100)}% on time`;
  const fontSize = compact ? "0.68rem" : "0.75rem";
  const padding = compact ? "0.1rem 0.4rem" : "0.2rem 0.55rem";

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.25rem",
        fontSize,
        fontWeight: 600,
        padding,
        borderRadius: "999px",
        backgroundColor: bg,
        color,
        border: `1px solid ${border}`,
        whiteSpace: "nowrap",
        lineHeight: 1.4,
      }}
    >
      <svg width={compact ? 10 : 12} height={compact ? 10 : 12} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
        <polyline points="9 22 9 12 15 12 15 22" />
      </svg>
      {text}
    </span>
  );
}
