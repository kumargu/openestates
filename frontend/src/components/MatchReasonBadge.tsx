import { useState } from "react";
import type { MatchReason } from "../lib/types.ts";

const SOURCE_COLORS: Record<string, { bg: string; color: string; border: string }> = {
  Rera:   { bg: "#edf7ed", color: "#2a7a2a", border: "#c8e6c8" },
  Reddit: { bg: "#f0f4ff", color: "#3b5998", border: "#c8d8f0" },
  Google: { bg: "#f0f4ff", color: "#3b5998", border: "#c8d8f0" },
  Llm:    { bg: "#f5f0fa", color: "#6b3fa0", border: "#d8c8e8" },
  Seed:   { bg: "#f5f5f5", color: "#666", border: "#e0e0e0" },
};

function confidenceLabel(c: number): string {
  if (c >= 0.9) return "verified";
  if (c >= 0.7) return `${Math.round(c * 100)}% confident`;
  if (c >= 0.5) return `${Math.round(c * 100)}% confident`;
  return "low confidence";
}

export function MatchReasonBadge({ reason }: { reason: MatchReason }) {
  const [hovered, setHovered] = useState(false);
  const style = SOURCE_COLORS[reason.source_type] || SOURCE_COLORS.Seed;

  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        gap: "0.1rem",
        padding: "0.35rem 0.6rem",
        borderRadius: "8px",
        backgroundColor: style.bg,
        border: `1px solid ${style.border}`,
        fontSize: "0.75rem",
        lineHeight: 1.4,
        cursor: "default",
      }}
    >
      <span style={{ color: style.color, fontWeight: 500 }}>
        {reason.display}
      </span>
      <span style={{ color: "#999", fontSize: "0.68rem" }}>
        {reason.source_type} &middot; {confidenceLabel(reason.confidence)}
      </span>

      {/* Hover tooltip with full details */}
      {hovered && (
        <div
          style={{
            position: "absolute",
            bottom: "calc(100% + 4px)",
            left: 0,
            padding: "0.4rem 0.6rem",
            borderRadius: "6px",
            backgroundColor: "#1a1a1a",
            color: "#eee",
            fontSize: "0.7rem",
            lineHeight: 1.5,
            whiteSpace: "nowrap",
            zIndex: 10,
            pointerEvents: "none",
            boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
          }}
        >
          <div>Fact: {reason.fact_key}</div>
          <div>Scored via: {reason.scoring_method}</div>
          <div>Score contribution: {(reason.score * 100).toFixed(0)}%</div>
        </div>
      )}
    </div>
  );
}
