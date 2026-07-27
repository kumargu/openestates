/**
 * TrustBadge — shows data provenance as a colored pill.
 * Green for RERA-verified, amber for discovered/pending, yellow for self-reported.
 * Renders nothing for "legacy" or undefined root_source.
 */

type TrustBadgeProps = {
  rootSource?: string;
  compact?: boolean;
};

const BADGE_CONFIG: Record<string, { label: string; bg: string; color: string; border: string; icon: string }> = {
  rera: {
    label: "RERA Verified",
    bg: "#f0fdf4",
    color: "#15803d",
    border: "#bbf7d0",
    // Shield checkmark
    icon: "shield",
  },
  discovered: {
    label: "Verification Pending",
    bg: "#fffbeb",
    color: "#92400e",
    border: "#fcd34d",
    // Clock
    icon: "clock",
  },
  seller: {
    label: "Self-reported",
    bg: "#fefce8",
    color: "#854d0e",
    border: "#fde68a",
    // User
    icon: "user",
  },
};

function BadgeIcon({ type, size }: { type: string; size: number }) {
  if (type === "shield") {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        <polyline points="9 12 11 14 15 10" />
      </svg>
    );
  }
  if (type === "clock") {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="10" />
        <polyline points="12 6 12 12 16 14" />
      </svg>
    );
  }
  if (type === "user") {
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
        <circle cx="12" cy="7" r="4" />
      </svg>
    );
  }
  return null;
}

export function TrustBadge({ rootSource, compact = false }: TrustBadgeProps) {
  if (!rootSource || rootSource === "legacy") return null;

  const config = BADGE_CONFIG[rootSource];
  if (!config) return null;

  const iconSize = compact ? 10 : 12;
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
        backgroundColor: config.bg,
        color: config.color,
        border: `1px solid ${config.border}`,
        whiteSpace: "nowrap",
        lineHeight: 1.4,
      }}
    >
      <BadgeIcon type={config.icon} size={iconSize} />
      {config.label}
    </span>
  );
}
