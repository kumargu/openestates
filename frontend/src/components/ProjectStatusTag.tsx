/**
 * ProjectStatusTag — renders project status as a colored tag.
 * Uses display_template text from the backend when available,
 * falls back to possession_status from seed data.
 */

import {
  resolveBuyerProjectStatus,
  type BuyerProjectStatusKey,
} from "../lib/projectStatus.ts";

type ProjectStatusTagProps = {
  status?: string;
  displayText?: string;
  possessionStatus?: string;
};

const STATUS_COLORS: Record<BuyerProjectStatusKey, { bg: string; color: string; border: string }> = {
  ready_to_move: { bg: "#f0fdf4", color: "#15803d", border: "#bbf7d0" },
  under_construction: { bg: "#eff6ff", color: "#1d4ed8", border: "#bfdbfe" },
  new_launch: { bg: "#faf5ff", color: "#7c3aed", border: "#ddd6fe" },
  delayed: { bg: "#fffbeb", color: "#92400e", border: "#fcd34d" },
  upcoming: { bg: "#f9fafb", color: "#6b7280", border: "#e5e7eb" },
};

export function ProjectStatusTag({ status, displayText, possessionStatus }: ProjectStatusTagProps) {
  const resolved = resolveBuyerProjectStatus({ status, displayText, possessionStatus });
  if (!resolved) return null;
  const colors = STATUS_COLORS[resolved.key];

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
      {resolved.label}
    </span>
  );
}
