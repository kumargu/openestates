import type { TransparencyScore } from "../lib/types.ts";

function scoreColor(score: number): string {
  if (score >= 80) return "var(--color-positive)";
  if (score >= 60) return "#3a8a3a";
  if (score >= 40) return "var(--color-warning)";
  return "var(--color-negative)";
}

export function TransparencyScoreTile({ data }: { data: TransparencyScore }) {
  const color = scoreColor(data.overall);
  const radius = 40;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference - (data.overall / 100) * circumference;

  return (
    <div className="section-card transparency-tile" style={{ marginBottom: "1rem" }}>
      <div style={{
        fontSize: "0.62rem",
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        color: "var(--color-text-muted)",
        marginBottom: "0.75rem",
      }}>
        Transparency Score
      </div>

      {/* Circular gauge + score */}
      <div style={{ display: "flex", alignItems: "center", gap: "1rem", marginBottom: "1rem" }}>
        <div style={{ position: "relative", width: "90px", height: "90px", flexShrink: 0 }}>
          <svg width="90" height="90" viewBox="0 0 100 100">
            {/* Background track */}
            <circle
              cx="50" cy="50" r={radius}
              fill="none"
              stroke="var(--color-bg-elevated)"
              strokeWidth="8"
            />
            {/* Progress arc */}
            <circle
              cx="50" cy="50" r={radius}
              fill="none"
              stroke={color}
              strokeWidth="8"
              strokeLinecap="round"
              strokeDasharray={circumference}
              strokeDashoffset={dashOffset}
              transform="rotate(-90 50 50)"
              style={{ transition: "stroke-dashoffset 0.8s var(--ease-out)" }}
            />
          </svg>
          <div style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
          }}>
            <span style={{ fontSize: "1.4rem", fontWeight: 700, color, lineHeight: 1 }}>
              {Math.round(data.overall)}
            </span>
            <span style={{ fontSize: "0.6rem", color: "var(--color-text-muted)" }}>/100</span>
          </div>
        </div>

        <p style={{
          margin: 0,
          fontSize: "0.82rem",
          color: "var(--color-text-secondary)",
          lineHeight: 1.5,
        }}>
          {data.explainer}
        </p>
      </div>

      {/* Component mini bars */}
      <div style={{ display: "grid", gap: "0.5rem" }}>
        {data.components.map((c) => {
          const pct = c.max_score > 0 ? (c.score / c.max_score) * 100 : 0;
          const barColor = pct >= 70 ? "var(--color-positive)" : pct >= 40 ? "var(--color-warning)" : "var(--color-negative)";
          return (
            <div key={c.label}>
              <div style={{
                display: "flex",
                justifyContent: "space-between",
                fontSize: "0.72rem",
                color: "var(--color-text-secondary)",
                marginBottom: "0.2rem",
              }}>
                <span>{c.label}</span>
                <span style={{ fontWeight: 600, color: barColor }}>
                  {Math.round(c.score)}/{Math.round(c.max_score)}
                </span>
              </div>
              <div style={{
                height: "4px",
                backgroundColor: "var(--color-bg-elevated)",
                borderRadius: "2px",
                overflow: "hidden",
              }}>
                <div style={{
                  height: "100%",
                  width: `${pct}%`,
                  backgroundColor: barColor,
                  borderRadius: "2px",
                  transition: "width 0.8s var(--ease-out)",
                }} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
