import type { AreaListItem } from "../lib/types.ts";

export function AreaCard({ area }: { area: AreaListItem }) {
  const trendColor = area.trend_direction === "up" ? "#2a7a2a" : "#b33";
  const trendArrow = area.trend_direction === "up" ? "^" : "v";

  return (
    <div
      style={{
        border: "1px solid #e0e0e0",
        borderRadius: "8px",
        padding: "1.25rem",
        backgroundColor: "#fafafa",
      }}
    >
      <h3 style={{ margin: "0 0 0.5rem", fontSize: "1rem" }}>{area.name}</h3>
      <p style={{ margin: "0 0 0.25rem", fontSize: "0.9rem", color: "#555" }}>
        {area.median_price_per_sqft.toLocaleString("en-IN")} /sqft{" "}
        <span style={{ color: trendColor, fontWeight: 600 }}>
          {trendArrow} {area.trend_direction}
        </span>
      </p>
      {area.primary_signal && (
        <span
          style={{
            fontSize: "0.75rem",
            padding: "0.15rem 0.5rem",
            borderRadius: "12px",
            backgroundColor: "#eef4ff",
            color: "#336",
            border: "1px solid #cce",
          }}
        >
          {area.primary_signal.replace(/_/g, " ")}
        </span>
      )}
    </div>
  );
}
