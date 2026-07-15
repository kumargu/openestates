import { useState, type PointerEvent } from "react";
import { formatCurrency, type PlanProjection } from "./model.ts";

export type PlanScenarioId = "buy" | "rent";
export type PlanGraphMetric = "netWorth" | "monthlyOutflow";

type PlanGraphProps = {
  projection: PlanProjection;
  horizon: number;
  metric: PlanGraphMetric;
  selected: PlanScenarioId;
  purchaseYear: number;
  onHorizonChange: (year: number) => void;
  onPreviewYearChange: (year: number | null) => void;
  onSelect: (scenario: PlanScenarioId) => void;
};

type GraphSeries = {
  id: PlanScenarioId;
  label: string;
  values: number[];
};

const GRAPH_WIDTH = 1080;
const GRAPH_HEIGHT = 420;
const GRAPH_INSET = { left: 72, right: 28, top: 36, bottom: 44 };

function graphSeries(projection: PlanProjection, metric: PlanGraphMetric): GraphSeries[] {
  if (metric === "monthlyOutflow") {
    return [
      { id: "buy", label: "Buy", values: projection.points.map((point) => point.annualEmi / 12) },
      { id: "rent", label: "Rent", values: projection.points.map((point) => point.annualRent / 12) },
    ];
  }

  return [
    { id: "buy", label: "Buy this home", values: projection.points.map((point) => point.buyNetWorth) },
    { id: "rent", label: "Rent + mutual funds", values: projection.points.map((point) => point.rentNetWorth) },
  ];
}

function gapPath(
  buyValues: number[],
  rentValues: number[],
  endYear: number,
  x: (year: number) => number,
  y: (value: number) => number,
): string {
  const segments: string[] = [];
  for (let year = 0; year <= endYear; year += 1) {
    const buyY = y(buyValues[year] ?? 0);
    const rentY = y(rentValues[year] ?? 0);
    segments.push(`${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${buyY.toFixed(1)}`);
  }
  for (let year = endYear; year >= 0; year -= 1) {
    segments.push(`L${x(year).toFixed(1)},${y(rentValues[year] ?? 0).toFixed(1)}`);
  }
  return `${segments.join(" ")} Z`;
}

export function PlanGraph({
  projection,
  horizon,
  metric,
  selected,
  purchaseYear,
  onHorizonChange,
  onPreviewYearChange,
  onSelect,
}: PlanGraphProps) {
  const [hoverYear, setHoverYear] = useState<number | null>(null);
  const series = graphSeries(projection, metric);
  const maxYear = projection.points.length - 1;
  const activeYear = Math.min(hoverYear ?? horizon, maxYear);
  const plotWidth = GRAPH_WIDTH - GRAPH_INSET.left - GRAPH_INSET.right;
  const plotHeight = GRAPH_HEIGHT - GRAPH_INSET.top - GRAPH_INSET.bottom;
  const maxValue = Math.max(1, ...series.flatMap((item) => item.values)) * 1.08;
  const x = (year: number) => GRAPH_INSET.left + (year / maxYear) * plotWidth;
  const y = (value: number) => GRAPH_INSET.top + plotHeight - (value / maxValue) * plotHeight;
  const line = (values: number[]) => values
    .map((value, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(value).toFixed(1)}`)
    .join(" ");
  const area = (values: number[]) => `${line(values)} L${x(maxYear).toFixed(1)},${GRAPH_INSET.top + plotHeight} L${x(0).toFixed(1)},${GRAPH_INSET.top + plotHeight} Z`;
  const selectedSeries = series.find((item) => item.id === selected) ?? series[0];
  const buyValues = series[0].values;
  const rentValues = series[1].values;
  const buyValue = buyValues[activeYear] ?? 0;
  const rentValue = rentValues[activeYear] ?? 0;
  const cursorX = x(activeYear);
  const buyWins = buyValue >= rentValue;
  const gapFillId = buyWins ? "home-plan-gap-buy" : "home-plan-gap-rent";
  const tooltipX = cursorX > GRAPH_WIDTH - 240 ? cursorX - 220 : cursorX + 18;
  const tooltipY = Math.max(56, Math.min(GRAPH_HEIGHT - 124, y(selectedSeries.values[activeYear] ?? 0) - 50));
  const leadValue = Math.abs(buyValue - rentValue);
  const leadLabel = metric === "netWorth"
    ? (buyWins ? "Buy leads" : "Rent + MF leads")
    : (buyValue <= rentValue ? "Buy costs less" : "Rent costs less");
  const breakEvenYear = projection.breakEvenYear;
  const loanFreeYear = projection.points.find((point, index, points) => (
    point.year > purchaseYear
    && point.loanBalance <= 0
    && (points[index - 1]?.loanBalance ?? 0) > 0
  ))?.year ?? null;
  const milestones = [
    { year: purchaseYear, label: purchaseYear === 0 ? "Purchase" : "Planned purchase", shortLabel: "H" },
    ...(metric === "netWorth" && breakEvenYear !== null
      ? [{ year: breakEvenYear, label: "Break-even", shortLabel: "=" }]
      : []),
    ...(metric === "netWorth" && loanFreeYear !== null
      ? [{ year: loanFreeYear, label: "Loan free", shortLabel: "✓" }]
      : []),
  ].filter((milestone, index, all) => milestone.year <= maxYear && all.findIndex((item) => item.year === milestone.year) === index);

  const updateHoverYear = (event: PointerEvent<SVGSVGElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    const svgX = ((event.clientX - bounds.left) / bounds.width) * GRAPH_WIDTH;
    const nextYear = Math.round(((svgX - GRAPH_INSET.left) / plotWidth) * maxYear);
    const boundedYear = Math.max(0, Math.min(maxYear, nextYear));
    setHoverYear(boundedYear);
    onPreviewYearChange(boundedYear);
  };

  const clearHoverYear = () => {
    setHoverYear(null);
    onPreviewYearChange(null);
  };

  return (
    <div className="home-plan-graph">
      <div className="home-plan-graph-legend">
        {series.map((item) => (
          <button
            type="button"
            key={item.id}
            className={selected === item.id ? "is-selected" : ""}
            onClick={() => onSelect(item.id)}
          >
            <i className={`home-plan-legend-dot home-plan-legend-dot--${item.id}`} />
            <span>
              <strong>{item.label}</strong>
              <small>{formatCurrency(item.values[activeYear] ?? 0, true)}</small>
            </span>
          </button>
        ))}
        <p>{hoverYear === null ? "Scrub the chart to preview years" : `Year ${activeYear} · click to pin`}</p>
      </div>

      <svg
        className="home-plan-graph-svg"
        viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
        role="img"
        aria-label={`${metric === "netWorth" ? "Projected net worth" : "Projected monthly outflow"} over ${maxYear} years`}
        onPointerMove={updateHoverYear}
        onPointerLeave={clearHoverYear}
        onClick={() => onHorizonChange(activeYear)}
      >
        <defs>
          <linearGradient id="home-plan-buy-area" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#e07858" stopOpacity=".22" />
            <stop offset="100%" stopColor="#e07858" stopOpacity=".02" />
          </linearGradient>
          <linearGradient id="home-plan-rent-area" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#6ba3c4" stopOpacity=".22" />
            <stop offset="100%" stopColor="#6ba3c4" stopOpacity=".02" />
          </linearGradient>
          <linearGradient id="home-plan-gap-buy" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#d87355" stopOpacity=".18" />
            <stop offset="100%" stopColor="#d87355" stopOpacity=".04" />
          </linearGradient>
          <linearGradient id="home-plan-gap-rent" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#79a6b8" stopOpacity=".18" />
            <stop offset="100%" stopColor="#79a6b8" stopOpacity=".04" />
          </linearGradient>
          <filter id="home-plan-tooltip-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="10" stdDeviation="12" floodColor="#1a1410" floodOpacity=".12" />
          </filter>
          <filter id="home-plan-cursor-glow">
            <feGaussianBlur stdDeviation="2" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>

        {[0.25, 0.5, 0.75, 1].map((ratio) => {
          const value = maxValue * ratio;
          const lineY = y(value);
          return (
            <g key={ratio}>
              <line x1={GRAPH_INSET.left} x2={GRAPH_WIDTH - GRAPH_INSET.right} y1={lineY} y2={lineY} className="home-plan-gridline" />
              <text x={GRAPH_INSET.left - 12} y={lineY + 4} className="home-plan-axis-label home-plan-axis-label--y">{formatCurrency(value, true)}</text>
            </g>
          );
        })}
        {[0, 5, 10, 15, 20].filter((year) => year <= maxYear).map((year) => (
          <text key={year} x={x(year)} y={GRAPH_HEIGHT - 12} className="home-plan-axis-label home-plan-axis-label--x">
            {year === 0 ? "Now" : `${year}y`}
          </text>
        ))}

        {metric === "netWorth" && (
          <path
            d={gapPath(buyValues, rentValues, activeYear, x, y)}
            className="home-plan-graph-gap"
            fill={`url(#${gapFillId})`}
          />
        )}

        <path d={area(selectedSeries.values)} className={`home-plan-graph-area home-plan-graph-area--${selected}`} />
        {series.map((item) => (
          <path
            key={item.id}
            d={line(item.values)}
            className={`home-plan-graph-line home-plan-graph-line--${item.id} ${selected === item.id ? "is-selected" : ""}`}
          />
        ))}

        {milestones.map((milestone) => {
          const eventValue = selectedSeries.values[milestone.year] ?? 0;
          return (
            <g
              key={`${milestone.year}-${milestone.label}`}
              className="home-plan-graph-event"
              transform={`translate(${x(milestone.year)} ${y(eventValue)})`}
              onClick={(event) => {
                event.stopPropagation();
                onHorizonChange(milestone.year);
              }}
            >
              <circle r="11" />
              <text y="3.5">{milestone.shortLabel}</text>
              <text y="-20" className="home-plan-graph-event-label">{milestone.label}</text>
            </g>
          );
        })}

        <g filter="url(#home-plan-cursor-glow)">
          <line x1={cursorX} x2={cursorX} y1={GRAPH_INSET.top} y2={GRAPH_INSET.top + plotHeight} className="home-plan-cursor" />
        </g>
        <text x={cursorX} y={GRAPH_INSET.top - 12} className="home-plan-cursor-label">Year {activeYear}</text>
        {series.map((item) => (
          <circle
            key={item.id}
            cx={cursorX}
            cy={y(item.values[activeYear] ?? 0)}
            r={selected === item.id ? 7 : 5}
            className={`home-plan-graph-point home-plan-graph-point--${item.id}`}
          />
        ))}

        <g className="home-plan-graph-tooltip" transform={`translate(${tooltipX} ${tooltipY})`} filter="url(#home-plan-tooltip-shadow)">
          <rect width="204" height="108" rx="14" />
          <text x="16" y="22" className="home-plan-tooltip-year">Year {activeYear}</text>
          <circle cx="19" cy="42" r="4.5" className="home-plan-tooltip-buy" />
          <text x="32" y="45">{series[0].label}</text>
          <text x="190" y="45" className="home-plan-tooltip-value">{formatCurrency(buyValue, true)}</text>
          <circle cx="19" cy="64" r="4.5" className="home-plan-tooltip-rent" />
          <text x="32" y="67">{series[1].label}</text>
          <text x="190" y="67" className="home-plan-tooltip-value">{formatCurrency(rentValue, true)}</text>
          <line x1="16" x2="190" y1="80" y2="80" />
          <text x="16" y="98" className="home-plan-tooltip-lead">{leadLabel}</text>
          <text x="190" y="98" className="home-plan-tooltip-value">{formatCurrency(leadValue, true)}</text>
        </g>
      </svg>
    </div>
  );
}
