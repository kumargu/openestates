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

const GRAPH_WIDTH = 960;
const GRAPH_HEIGHT = 340;
const GRAPH_INSET = { left: 70, right: 24, top: 28, bottom: 38 };

function graphSeries(projection: PlanProjection, metric: PlanGraphMetric): GraphSeries[] {
  if (metric === "monthlyOutflow") {
    return [
      { id: "buy", label: "Buy this home", values: projection.points.map((point) => point.annualEmi / 12) },
      { id: "rent", label: "Rent", values: projection.points.map((point) => point.annualRent / 12) },
    ];
  }

  return [
    { id: "buy", label: "Buy this home", values: projection.points.map((point) => point.buyNetWorth) },
    { id: "rent", label: "Rent + mutual funds", values: projection.points.map((point) => point.rentNetWorth) },
  ];
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
  const buyValue = series[0].values[activeYear] ?? 0;
  const rentValue = series[1].values[activeYear] ?? 0;
  const cursorX = x(activeYear);
  const tooltipX = cursorX > GRAPH_WIDTH - 235 ? cursorX - 212 : cursorX + 16;
  const tooltipY = Math.max(50, Math.min(GRAPH_HEIGHT - 118, y(selectedSeries.values[activeYear] ?? 0) - 46));
  const leadValue = Math.abs(buyValue - rentValue);
  const leadLabel = metric === "netWorth"
    ? (buyValue >= rentValue ? "Buy leads" : "Rent + MF leads")
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
              <small>{formatCurrency(item.values[activeYear] ?? 0, true)} at year {activeYear}</small>
            </span>
          </button>
        ))}
        <p>{hoverYear === null ? "Move across the chart to explore. Click to keep a year." : `Previewing year ${activeYear} · click to keep`}</p>
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
            <stop offset="0%" stopColor="#ed8465" stopOpacity=".2" />
            <stop offset="100%" stopColor="#ed8465" stopOpacity=".01" />
          </linearGradient>
          <linearGradient id="home-plan-rent-area" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#68a8d4" stopOpacity=".2" />
            <stop offset="100%" stopColor="#68a8d4" stopOpacity=".01" />
          </linearGradient>
          <filter id="home-plan-tooltip-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="8" stdDeviation="10" floodColor="#172123" floodOpacity=".14" />
          </filter>
        </defs>

        {[0.25, 0.5, 0.75, 1].map((ratio) => {
          const value = maxValue * ratio;
          const lineY = y(value);
          return (
            <g key={ratio}>
              <line x1={GRAPH_INSET.left} x2={GRAPH_WIDTH - GRAPH_INSET.right} y1={lineY} y2={lineY} className="home-plan-gridline" />
              <text x={GRAPH_INSET.left - 10} y={lineY + 3} className="home-plan-axis-label home-plan-axis-label--y">{formatCurrency(value, true)}</text>
            </g>
          );
        })}
        {[0, 5, 10, 15, 20].filter((year) => year <= maxYear).map((year) => (
          <text key={year} x={x(year)} y={GRAPH_HEIGHT - 9} className="home-plan-axis-label home-plan-axis-label--x">
            {year === 0 ? "Now" : `${year}y`}
          </text>
        ))}

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
              <circle r="10" />
              <text y="3">{milestone.shortLabel}</text>
              <text y="-17" className="home-plan-graph-event-label">{milestone.label}</text>
            </g>
          );
        })}

        <line x1={cursorX} x2={cursorX} y1={GRAPH_INSET.top} y2={GRAPH_INSET.top + plotHeight} className="home-plan-cursor" />
        <text x={cursorX} y={GRAPH_INSET.top - 9} className="home-plan-cursor-label">YEAR {activeYear}</text>
        {series.map((item) => (
          <circle
            key={item.id}
            cx={cursorX}
            cy={y(item.values[activeYear] ?? 0)}
            r={selected === item.id ? 6 : 4}
            className={`home-plan-graph-point home-plan-graph-point--${item.id}`}
          />
        ))}

        <g className="home-plan-graph-tooltip" transform={`translate(${tooltipX} ${tooltipY})`} filter="url(#home-plan-tooltip-shadow)">
          <rect width="196" height="104" rx="13" />
          <text x="14" y="20" className="home-plan-tooltip-year">YEAR {activeYear}</text>
          <circle cx="17" cy="40" r="4" className="home-plan-tooltip-buy" />
          <text x="29" y="43">{series[0].label}</text>
          <text x="182" y="43" className="home-plan-tooltip-value">{formatCurrency(buyValue, true)}</text>
          <circle cx="17" cy="61" r="4" className="home-plan-tooltip-rent" />
          <text x="29" y="64">{series[1].label}</text>
          <text x="182" y="64" className="home-plan-tooltip-value">{formatCurrency(rentValue, true)}</text>
          <line x1="14" x2="182" y1="76" y2="76" />
          <text x="14" y="94" className="home-plan-tooltip-lead">{leadLabel}</text>
          <text x="182" y="94" className="home-plan-tooltip-value">{formatCurrency(leadValue, true)}</text>
        </g>
      </svg>

      <label className="home-plan-year-scrubber">
        <span>Now</span>
        <input
          type="range"
          min={0}
          max={maxYear}
          step={1}
          value={horizon}
          onChange={(event) => onHorizonChange(Number(event.target.value))}
          aria-label="Projection horizon"
        />
        <span>{maxYear} years</span>
      </label>
    </div>
  );
}
