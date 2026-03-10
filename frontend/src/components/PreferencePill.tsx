import type { PreferenceCoverage } from "../lib/types.ts";

const STATUS_STYLES: Record<string, { bg: string; color: string; border: string; icon: string }> = {
  matched: { bg: "#edf7ed", color: "#2a7a2a", border: "#c8e6c8", icon: "\u2713" },
  partial: { bg: "#fdf5e6", color: "#8a6d00", border: "#e8d8a0", icon: "\u25D0" },
  no_data: { bg: "#f5f5f5", color: "#999", border: "#e0e0e0", icon: "?" },
};

export function PreferencePill({ coverage }: { coverage: PreferenceCoverage }) {
  const style = STATUS_STYLES[coverage.status] || STATUS_STYLES.no_data;

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "0.25rem",
        padding: "0.2rem 0.5rem",
        borderRadius: "6px",
        fontSize: "0.72rem",
        fontWeight: 500,
        backgroundColor: style.bg,
        color: style.color,
        border: `1px solid ${style.border}`,
        whiteSpace: "nowrap",
      }}
    >
      <span style={{ fontSize: "0.7rem" }}>{style.icon}</span>
      {coverage.preference}
    </span>
  );
}
