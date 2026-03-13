/**
 * ProjectStatusTag — renders project status as a colored tag.
 * Uses display_template text from the backend when available,
 * falls back to possession_status from seed data.
 */

type ProjectStatusTagProps = {
  status?: string;
  displayText?: string;
  possessionStatus?: string;
};

const STATUS_COLORS: Record<string, { bg: string; color: string; border: string }> = {
  ready_to_move: { bg: "#f0fdf4", color: "#15803d", border: "#bbf7d0" },
  under_construction: { bg: "#eff6ff", color: "#1d4ed8", border: "#bfdbfe" },
  new_launch: { bg: "#faf5ff", color: "#7c3aed", border: "#ddd6fe" },
  delayed: { bg: "#fffbeb", color: "#92400e", border: "#fcd34d" },
  upcoming: { bg: "#f9fafb", color: "#6b7280", border: "#e5e7eb" },
};

// Map possession_status values from seed data to status keys
function normalizePossessionStatus(ps: string): string {
  const lower = ps.toLowerCase().replace(/[_\s-]+/g, "_");
  if (lower === "ready" || lower === "ready_to_move") return "ready_to_move";
  if (lower.includes("construction") || lower === "under_construction") return "under_construction";
  if (lower.includes("new_launch") || lower === "new_launch") return "new_launch";
  if (lower.includes("delay")) return "delayed";
  if (lower.includes("upcoming")) return "upcoming";
  return lower;
}

// Fallback display text when display_template is not available
function fallbackDisplayText(status: string): string {
  switch (status) {
    case "ready_to_move": return "Ready to move";
    case "under_construction": return "Under construction";
    case "new_launch": return "New launch";
    case "delayed": return "Delayed";
    case "upcoming": return "Upcoming";
    default: return status.replace(/_/g, " ");
  }
}

export function ProjectStatusTag({ status, displayText, possessionStatus }: ProjectStatusTagProps) {
  // Determine the status key for coloring
  const statusKey = status
    ? status
    : possessionStatus
      ? normalizePossessionStatus(possessionStatus)
      : null;

  if (!statusKey) return null;

  // Determine the display text
  const text = displayText || fallbackDisplayText(statusKey);

  const colors = STATUS_COLORS[statusKey] || STATUS_COLORS["upcoming"];

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontSize: "0.72rem",
        fontWeight: 500,
        padding: "0.15rem 0.45rem",
        borderRadius: "6px",
        backgroundColor: colors.bg,
        color: colors.color,
        border: `1px solid ${colors.border}`,
        whiteSpace: "nowrap",
        lineHeight: 1.4,
      }}
    >
      {text}
    </span>
  );
}
