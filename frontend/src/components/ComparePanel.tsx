/**
 * Side-by-side comparison panel that slides in from the right.
 * Wider than the quick-view panel. Shows 2-3 properties in columns.
 */
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyDetailResponse, PropertyCard as PropertyCardType } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function scoreColor(v: number): string {
  if (v >= 0.7) return "var(--color-positive)";
  if (v >= 0.4) return "var(--color-warning)";
  return "var(--color-negative)";
}

function scoreLabel(v: number): string {
  return `${Math.round(v * 100)}%`;
}

/** Find which property has the best value for a given metric */
function bestIdx(details: (PropertyDetailResponse | null)[], getter: (d: PropertyDetailResponse) => number, higher = true): number {
  let best = -1;
  let bestVal = higher ? -Infinity : Infinity;
  details.forEach((d, i) => {
    if (!d) return;
    const v = getter(d);
    if ((higher && v > bestVal) || (!higher && v < bestVal)) {
      bestVal = v;
      best = i;
    }
  });
  return best;
}

type CompareRowProps = {
  label: string;
  values: { text: string; color?: string; isBest?: boolean }[];
};

function CompareRow({ label, values }: CompareRowProps) {
  return (
    <div className="cmp-row">
      <div className="cmp-row-label">{label}</div>
      <div className="cmp-row-values">
        {values.map((v, i) => (
          <div key={i} className={`cmp-row-value ${v.isBest ? "cmp-row-value--best" : ""}`} style={{ color: v.color }}>
            {v.text}
          </div>
        ))}
      </div>
    </div>
  );
}

type ScoreRowProps = {
  label: string;
  scores: { value: number; isBest: boolean }[];
};

function ScoreRow({ label, scores }: ScoreRowProps) {
  return (
    <div className="cmp-row">
      <div className="cmp-row-label">{label}</div>
      <div className="cmp-row-values">
        {scores.map((s, i) => (
          <div key={i} className={`cmp-row-value ${s.isBest ? "cmp-row-value--best" : ""}`}>
            <div className="cmp-score-bar-wrap">
              <div className="cmp-score-bar-track">
                <div
                  className="cmp-score-bar-fill"
                  style={{ width: `${Math.round(s.value * 100)}%`, backgroundColor: scoreColor(s.value) }}
                />
              </div>
              <span className="cmp-score-bar-text" style={{ color: scoreColor(s.value) }}>
                {scoreLabel(s.value)}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

type Props = {
  cards: PropertyCardType[];
  onClose: () => void;
  onRemove: (id: string) => void;
};

export function ComparePanel({ cards, onClose, onRemove }: Props) {
  const [details, setDetails] = useState<(PropertyDetailResponse | null)[]>(cards.map(() => null));
  const [loading, setLoading] = useState(true);
  const [closing, setClosing] = useState(false);

  useEffect(() => {
    setLoading(true);
    Promise.all(cards.map((c) => getProperty(c.id).catch(() => null)))
      .then((results) => {
        setDetails(results);
        setLoading(false);
      });
  }, [cards.map(c => c.id).join(",")]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  const handleClose = () => {
    setClosing(true);
    setTimeout(onClose, 250);
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) handleClose();
  };

  const colCount = cards.length;

  // Precompute "best" indices for key metrics
  const bestPrice = bestIdx(details, d => d.property.price_per_sqft, false);
  const bestSociety = bestIdx(details, d => d.property.society_quality_score);
  const bestBuilder = bestIdx(details, d => d.property.builder_quality_score);
  const bestDocs = bestIdx(details, d => d.property.document_completeness_score);
  const bestRisk = bestIdx(details, d => d.property.litigation_risk, false);
  const bestGreenery = bestIdx(details, d => d.property.greenery_score ?? 0);
  const bestResale = bestIdx(details, d => d.property.resale_strength_score ?? 0);

  return (
    <div
      className={`side-panel-backdrop ${closing ? "side-panel-backdrop--closing" : ""}`}
      onClick={handleBackdropClick}
    >
      <div
        className={`compare-panel ${closing ? "compare-panel--closing" : ""}`}
        role="dialog"
        aria-label="Compare properties"
      >
        {/* Header */}
        <div className="side-panel-header">
          <button className="side-panel-close" onClick={handleClose} aria-label="Close comparison">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
          <span className="side-panel-header-label">Compare {colCount} properties</span>
        </div>

        {/* Scrollable body */}
        <div className="compare-panel-body">
          {/* Property header cards */}
          <div className="cmp-headers" style={{ gridTemplateColumns: `repeat(${colCount}, 1fr)` }}>
            {cards.map((card) => (
              <div key={card.id} className="cmp-header-card">
                <div className="cmp-header-img">
                  <ImageWithFallback
                    src={card.hero_image}
                    alt={card.title}
                    style={{ width: "100%", height: "100%", objectFit: "cover" }}
                  />
                  <button
                    className="cmp-header-remove"
                    onClick={() => onRemove(card.id)}
                    aria-label={`Remove ${card.title}`}
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                      <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                </div>
                <h3 className="cmp-header-title">{card.title}</h3>
                <p className="cmp-header-location">{card.society_name} &middot; {card.area}</p>
                <Link to={`/property/${card.id}`} className="cmp-header-link">Full details</Link>
              </div>
            ))}
          </div>

          {/* Loading state */}
          {loading && (
            <div className="side-panel-loading" style={{ padding: "1.5rem" }}>
              <div className="side-panel-loading-bar" />
              <div className="side-panel-loading-bar" style={{ width: "60%" }} />
              <div className="side-panel-loading-bar" style={{ width: "80%" }} />
            </div>
          )}

          {/* Comparison rows */}
          {!loading && (
            <div className="cmp-section">
              {/* Basics */}
              <h3 className="cmp-section-title">Basics</h3>
              <CompareRow
                label="Price"
                values={cards.map((c, i) => ({
                  text: formatPrice(c.price),
                  isBest: i === bestPrice,
                }))}
              />
              <CompareRow
                label="Price/sqft"
                values={cards.map((c, i) => ({
                  text: `\u20B9${c.price_per_sqft.toLocaleString("en-IN")}`,
                  isBest: i === bestPrice,
                }))}
              />
              <CompareRow
                label="Size"
                values={cards.map((c) => ({ text: `${c.bhk} BHK · ${c.sqft.toLocaleString()} sqft` }))}
              />
              <CompareRow
                label="Floor"
                values={cards.map((c) => ({ text: `${c.floor}/${c.total_floors}` }))}
              />
              <CompareRow
                label="Facing"
                values={cards.map((c) => ({ text: c.facing }))}
              />
              <CompareRow
                label="Possession"
                values={cards.map((c) => ({
                  text: c.possession_status === "ready" ? "Ready to move" : "Under construction",
                  color: c.possession_status === "ready" ? "var(--color-positive)" : "var(--color-text-secondary)",
                }))}
              />
              <CompareRow
                label="Metro"
                values={cards.map((c) => ({ text: `${c.metro_distance_mins} min` }))}
              />

              {/* Scores */}
              <h3 className="cmp-section-title" style={{ marginTop: "1.25rem" }}>Transparency scores</h3>
              <ScoreRow
                label="Society"
                scores={details.map((d, i) => ({
                  value: d?.property.society_quality_score ?? 0,
                  isBest: i === bestSociety,
                }))}
              />
              <ScoreRow
                label="Builder"
                scores={details.map((d, i) => ({
                  value: d?.property.builder_quality_score ?? 0,
                  isBest: i === bestBuilder,
                }))}
              />
              <ScoreRow
                label="Documents"
                scores={details.map((d, i) => ({
                  value: d?.property.document_completeness_score ?? 0,
                  isBest: i === bestDocs,
                }))}
              />
              <ScoreRow
                label="Greenery"
                scores={details.map((d, i) => ({
                  value: d?.property.greenery_score ?? 0,
                  isBest: i === bestGreenery,
                }))}
              />
              <ScoreRow
                label="Resale"
                scores={details.map((d, i) => ({
                  value: d?.property.resale_strength_score ?? 0,
                  isBest: i === bestResale,
                }))}
              />

              {/* Risk */}
              <CompareRow
                label="Litigation risk"
                values={details.map((d, i) => {
                  const risk = d?.property.litigation_risk ?? 0;
                  return {
                    text: risk <= 0.1 ? "Low" : risk <= 0.3 ? "Moderate" : "High",
                    color: risk <= 0.1 ? "var(--color-positive)" : risk <= 0.3 ? "var(--color-warning)" : "var(--color-negative)",
                    isBest: i === bestRisk,
                  };
                })}
              />

              {/* Maintenance */}
              <CompareRow
                label="Maintenance"
                values={details.map((d) => ({
                  text: d ? `\u20B9${d.property.maintenance_cost_monthly.toLocaleString("en-IN")}/mo` : "\u2014",
                }))}
              />

              {/* Tradeoffs */}
              {details.some(d => d?.tradeoffs) && (
                <>
                  <h3 className="cmp-section-title" style={{ marginTop: "1.25rem" }}>Strengths & cautions</h3>
                  <div className="cmp-tradeoffs" style={{ gridTemplateColumns: `repeat(${colCount}, 1fr)` }}>
                    {details.map((d, i) => (
                      <div key={i} className="cmp-tradeoff-col">
                        {d?.tradeoffs?.strengths.slice(0, 3).map((s, j) => (
                          <div key={`s-${j}`} className="cmp-tradeoff-item cmp-tradeoff-item--positive">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--color-positive)" strokeWidth="3" strokeLinecap="round"><polyline points="20 6 9 17 4 12" /></svg>
                            {s}
                          </div>
                        ))}
                        {d?.tradeoffs?.cautions.slice(0, 3).map((c, j) => (
                          <div key={`c-${j}`} className="cmp-tradeoff-item cmp-tradeoff-item--caution">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--color-warning)" strokeWidth="3" strokeLinecap="round"><circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /></svg>
                            {c}
                          </div>
                        ))}
                        {!d && <span className="cmp-tradeoff-empty">No data</span>}
                      </div>
                    ))}
                  </div>
                </>
              )}

              {/* Society */}
              {details.some(d => d?.society) && (
                <>
                  <h3 className="cmp-section-title" style={{ marginTop: "1.25rem" }}>Society</h3>
                  <div className="cmp-tradeoffs" style={{ gridTemplateColumns: `repeat(${colCount}, 1fr)` }}>
                    {details.map((d, i) => (
                      <div key={i} className="cmp-tradeoff-col">
                        {d?.society ? (
                          <p className="cmp-society-review">{d.society.review_summary}</p>
                        ) : (
                          <span className="cmp-tradeoff-empty">No data</span>
                        )}
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
