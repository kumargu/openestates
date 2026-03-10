import type { AreaIntelligence } from "../lib/types";

type InsightRow = { label: string; value: string };

function formatDate(dateStr: string): string {
  const separators = /[/\-]/;
  const parts = dateStr.split(separators);
  if (parts.length === 3) {
    if (parts[0].length === 4) {
      const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
      const m = parseInt(parts[1], 10);
      return `${parseInt(parts[2], 10)} ${months[m - 1]} ${parts[0]}`;
    }
    const day = parseInt(parts[0], 10);
    const month = parseInt(parts[1], 10);
    if (day > 0 && month >= 1 && month <= 12) {
      const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
      return `${day} ${months[month - 1]} ${parts[2]}`;
    }
  }
  return dateStr;
}

export function AreaIntelligenceTile({
  area,
  intelligence,
}: {
  area: string;
  intelligence: AreaIntelligence;
}) {
  // Build insight rows from available fields
  const rows: InsightRow[] = [];
  if (intelligence.safety) rows.push({ label: "Safety", value: intelligence.safety });
  if (intelligence.commute_reality) rows.push({ label: "Commute", value: intelligence.commute_reality });
  if (intelligence.water_supply) rows.push({ label: "Water Supply", value: intelligence.water_supply });
  if (intelligence.noise_level) rows.push({ label: "Noise Level", value: intelligence.noise_level });
  if (intelligence.green_cover) rows.push({ label: "Green Cover", value: intelligence.green_cover });
  if (intelligence.community_vibe) rows.push({ label: "Community", value: intelligence.community_vibe });
  if (intelligence.walkability) rows.push({ label: "Walkability", value: intelligence.walkability });
  if (intelligence.school_quality) rows.push({ label: "Schools", value: intelligence.school_quality });
  if (intelligence.grocery_shopping) rows.push({ label: "Groceries", value: intelligence.grocery_shopping });
  if (intelligence.healthcare_access) rows.push({ label: "Healthcare", value: intelligence.healthcare_access });

  const hasGems = intelligence.hidden_gems && intelligence.hidden_gems.length > 0;

  // Combine deal_breakers and recurring_complaints as "watch out" items
  const watchOutItems: string[] = [
    ...(intelligence.deal_breakers || []),
    ...(intelligence.recurring_complaints || []),
  ];
  const hasWatchOut = watchOutItems.length > 0;

  return (
    <div className="section-card" data-testid="area-intelligence-tile">
      {/* Header */}
      <div className="section-card-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="16" x2="12" y2="12" />
          <line x1="12" y1="8" x2="12.01" y2="8" />
        </svg>
        <h2>Area Intelligence &middot; {area}</h2>
      </div>

      {/* Source count */}
      {intelligence.source_count != null && (
        <p style={{
          margin: "0 0 1rem",
          fontSize: "0.82rem",
          color: "var(--color-text-muted)",
        }}>
          Based on {intelligence.source_count} Reddit discussions
        </p>
      )}

      {/* Overall sentiment */}
      {intelligence.overall_sentiment && (
        <p style={{
          margin: "0 0 1rem",
          fontSize: "0.95rem",
          lineHeight: 1.6,
          color: "var(--color-text)",
          fontWeight: 500,
        }}>
          {intelligence.overall_sentiment}
        </p>
      )}

      {/* Insight rows */}
      {rows.length > 0 && (
        <div style={{ marginBottom: "1.25rem" }}>
          {rows.map((row) => (
            <div
              key={row.label}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "baseline",
                padding: "0.55rem 0",
                borderBottom: "1px solid var(--color-border)",
                gap: "1rem",
              }}
            >
              <span style={{
                fontSize: "0.82rem",
                fontWeight: 500,
                color: "var(--color-text-muted)",
                flexShrink: 0,
              }}>
                {row.label}
              </span>
              <span style={{
                fontSize: "0.85rem",
                color: "var(--color-text)",
                textAlign: "right",
                lineHeight: 1.4,
              }}>
                {row.value}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Hidden gems + Watch out — side by side when both present */}
      {(hasGems || hasWatchOut) && (
        <div className="detail-grid" style={{ gap: "1rem", marginBottom: "0.75rem" }}>
          {hasGems && (
            <div style={{
              padding: "1rem",
              borderRadius: "var(--radius-sm)",
              backgroundColor: "var(--color-positive-bg)",
              border: "1px solid var(--color-positive-border)",
            }}>
              <h3 style={{
                margin: "0 0 0.5rem",
                fontSize: "0.7rem",
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "var(--color-positive)",
              }}>
                What residents love
              </h3>
              <ul className="insight-list insight-list-positive">
                {intelligence.hidden_gems!.map((item, i) => <li key={i}>{item}</li>)}
              </ul>
            </div>
          )}
          {hasWatchOut && (
            <div style={{
              padding: "1rem",
              borderRadius: "var(--radius-sm)",
              backgroundColor: "#fdf8f0",
              border: "1px solid #e8dcc0",
            }}>
              <h3 style={{
                margin: "0 0 0.5rem",
                fontSize: "0.7rem",
                fontWeight: 600,
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "var(--color-warning)",
              }}>
                Watch out for
              </h3>
              <ul style={{
                listStyle: "none",
                padding: 0,
                margin: 0,
              }}>
                {watchOutItems.map((item, i) => (
                  <li
                    key={i}
                    style={{
                      position: "relative",
                      padding: "0.6rem 0 0.6rem 1.5rem",
                      fontSize: "0.9rem",
                      color: "var(--color-text)",
                      lineHeight: 1.5,
                      borderBottom: i < watchOutItems.length - 1 ? "1px solid var(--color-border)" : "none",
                    }}
                  >
                    <span style={{
                      position: "absolute",
                      left: 0,
                      top: "0.7rem",
                      color: "var(--color-warning)",
                      fontWeight: 700,
                      fontSize: "0.85rem",
                    }}>
                      {"\u2013"}
                    </span>
                    {item}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}

      {/* Footer */}
      <div style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        paddingTop: "0.5rem",
        borderTop: "1px solid var(--color-border)",
      }}>
        <span style={{
          fontSize: "0.72rem",
          color: "var(--color-text-muted)",
        }}>
          Source: r/bangalore
          {intelligence.last_updated ? ` \u00B7 Updated ${formatDate(intelligence.last_updated)}` : ""}
        </span>
      </div>
    </div>
  );
}
