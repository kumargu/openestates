/**
 * ConfidenceMeter -- compact pill/bar showing confidence in data quality.
 * Green for High (>=0.7), amber for Moderate (>=0.4), gray for Low (<0.4).
 * Shows component breakdown on hover.
 * Renders nothing when confidence_score is undefined.
 */

import { useState } from "react";
import type { ConfidenceScore } from "../lib/types.ts";

type ConfidenceMeterProps = {
  confidence?: ConfidenceScore;
  compact?: boolean;
};

function getConfidenceConfig(overall: number): { bg: string; color: string; border: string; barColor: string } {
  if (overall >= 0.7) {
    return { bg: "#f0fdf4", color: "#15803d", border: "#bbf7d0", barColor: "#22c55e" };
  }
  if (overall >= 0.4) {
    return { bg: "#fffbeb", color: "#92400e", border: "#fcd34d", barColor: "#f59e0b" };
  }
  return { bg: "#f3f4f6", color: "#6b7280", border: "#d1d5db", barColor: "#9ca3af" };
}

export function ConfidenceMeter({ confidence, compact = false }: ConfidenceMeterProps) {
  const [showBreakdown, setShowBreakdown] = useState(false);

  if (!confidence) return null;

  const config = getConfidenceConfig(confidence.overall);
  const pct = Math.round(confidence.overall * 100);
  const fontSize = compact ? "0.68rem" : "0.75rem";
  const padding = compact ? "0.1rem 0.4rem" : "0.2rem 0.55rem";

  return (
    <span
      style={{ position: "relative", display: "inline-flex", alignItems: "center" }}
      onMouseEnter={() => setShowBreakdown(true)}
      onMouseLeave={() => setShowBreakdown(false)}
    >
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "0.3rem",
          fontSize,
          fontWeight: 600,
          padding,
          borderRadius: "999px",
          backgroundColor: config.bg,
          color: config.color,
          border: `1px solid ${config.border}`,
          whiteSpace: "nowrap",
          lineHeight: 1.4,
          cursor: "default",
        }}
      >
        {/* Mini bar */}
        <span
          style={{
            display: "inline-block",
            width: compact ? "20px" : "28px",
            height: compact ? "4px" : "5px",
            borderRadius: "3px",
            backgroundColor: `${config.barColor}30`,
            overflow: "hidden",
            position: "relative",
          }}
        >
          <span
            style={{
              display: "block",
              width: `${pct}%`,
              height: "100%",
              backgroundColor: config.barColor,
              borderRadius: "3px",
            }}
          />
        </span>
        {confidence.label}
      </span>

      {/* Hover breakdown tooltip */}
      {showBreakdown && (
        <div
          style={{
            position: "absolute",
            bottom: "calc(100% + 8px)",
            left: "50%",
            transform: "translateX(-50%)",
            padding: "0.6rem 0.8rem",
            borderRadius: "10px",
            backgroundColor: "#1a1a1a",
            color: "#eee",
            fontSize: "0.72rem",
            lineHeight: 1.5,
            zIndex: 20,
            pointerEvents: "none",
            boxShadow: "0 4px 16px rgba(0,0,0,0.2)",
            minWidth: "220px",
            maxWidth: "280px",
          }}
        >
          <div style={{ fontWeight: 600, marginBottom: "0.35rem", color: "#fff" }}>
            Data Confidence: {pct}%
          </div>
          {confidence.components.map((c) => (
            <div
              key={c.dimension}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: "0.5rem",
                marginBottom: "0.2rem",
              }}
            >
              <span style={{ color: "#bbb", textTransform: "capitalize" }}>
                {c.dimension.replace(/_/g, " ")}
              </span>
              <span style={{ fontWeight: 600 }}>
                {Math.round(c.score * 100)}%
              </span>
            </div>
          ))}
        </div>
      )}
    </span>
  );
}
