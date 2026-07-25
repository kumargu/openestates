import { useState, type PointerEvent } from "react";
import { formatCurrency, type PlanProjection } from "./model.ts";

type PlanGraphProps = {
  projection: PlanProjection;
  horizon: number;
  onHorizonChange: (year: number) => void;
  onPreviewYearChange: (year: number | null) => void;
};

type GraphSeries = {
  id: "buy" | "rent";
  label: string;
  values: number[];
};

const GRAPH_WIDTH = 1080;
const GRAPH_HEIGHT = 420;
const GRAPH_INSET = { left: 72, right: 172, top: 40, bottom: 44 };

function graphSeries(projection: PlanProjection): GraphSeries[] {
  return [
    { id: "buy", label: "Buy", values: projection.points.map((point) => point.buyNetWorth) },
    { id: "rent", label: "Rent + invest", values: projection.points.map((point) => point.rentNetWorth) },
  ];
}

export function PlanGraph({
  projection,
  horizon,
  onHorizonChange,
  onPreviewYearChange,
}: PlanGraphProps) {
  const [hoverYear, setHoverYear] = useState<number | null>(null);
  const series = graphSeries(projection);
  const maxYear = projection.points.length - 1;
  const activeYear = Math.min(hoverYear ?? horizon, maxYear);
  const plotWidth = GRAPH_WIDTH - GRAPH_INSET.left - GRAPH_INSET.right;
  const plotHeight = GRAPH_HEIGHT - GRAPH_INSET.top - GRAPH_INSET.bottom;
  const allValues = series.flatMap((item) => item.values);
  const rawMinValue = Math.min(0, ...allValues);
  const rawMaxValue = Math.max(1, ...allValues);
  const valuePadding = Math.max(1, rawMaxValue - rawMinValue) * 0.08;
  const minValue = rawMinValue < 0 ? rawMinValue - valuePadding : 0;
  const maxValue = rawMaxValue + valuePadding;
  const valueRange = maxValue - minValue;
  const x = (year: number) => GRAPH_INSET.left + (year / maxYear) * plotWidth;
  const y = (value: number) => (
    GRAPH_INSET.top + plotHeight - ((value - minValue) / valueRange) * plotHeight
  );
  const line = (values: number[]) => values
    .map((value, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(value).toFixed(1)}`)
    .join(" ");
  const buyValues = series[0].values;
  const rentValues = series[1].values;
  const buyValue = buyValues[activeYear] ?? 0;
  const rentValue = rentValues[activeYear] ?? 0;
  const cursorX = x(activeYear);
  const tooltipX = cursorX > GRAPH_WIDTH - 250 ? cursorX - 224 : cursorX + 18;
  const tooltipY = Math.max(48, Math.min(GRAPH_HEIGHT - 98, Math.min(y(buyValue), y(rentValue)) - 34));
  const finalBuyY = y(buyValues[maxYear] ?? 0);
  const finalRentY = y(rentValues[maxYear] ?? 0);
  const labelsAreClose = Math.abs(finalBuyY - finalRentY) < 30;
  const buyLabelY = labelsAreClose && finalBuyY >= finalRentY ? finalBuyY + 12 : finalBuyY;
  const rentLabelY = labelsAreClose && finalRentY >= finalBuyY ? finalRentY + 12 : finalRentY;

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
      <div className="home-plan-graph__heading">
        <h2>Accumulated wealth: buying vs renting</h2>
      </div>
      <svg
        className="home-plan-graph-svg"
        viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
        role="img"
        aria-label={`Projected wealth over ${maxYear} years`}
        onPointerMove={updateHoverYear}
        onPointerLeave={clearHoverYear}
        onClick={() => onHorizonChange(activeYear)}
      >
        <defs>
          <filter id="home-plan-tooltip-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="10" stdDeviation="12" floodColor="#1a1410" floodOpacity=".12" />
          </filter>
        </defs>

        {[0.25, 0.5, 0.75, 1].map((ratio) => {
          const value = minValue + valueRange * ratio;
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

        {series.map((item) => (
          <path
            key={`${item.id}-${Math.round(item.values[maxYear] ?? 0)}`}
            d={line(item.values)}
            pathLength="1"
            className={`home-plan-graph-line home-plan-graph-line--${item.id}`}
          />
        ))}

        <line x1={cursorX} x2={cursorX} y1={GRAPH_INSET.top} y2={GRAPH_INSET.top + plotHeight} className="home-plan-cursor" />
        <text x={cursorX} y={GRAPH_INSET.top - 12} className="home-plan-cursor-label">Year {activeYear}</text>
        {series.map((item) => (
          <circle
            key={item.id}
            cx={cursorX}
            cy={y(item.values[activeYear] ?? 0)}
            r="6"
            className={`home-plan-graph-point home-plan-graph-point--${item.id}`}
          />
        ))}

        <g className="home-plan-graph-end-label home-plan-graph-end-label--buy" transform={`translate(${x(maxYear) + 14} ${buyLabelY})`}>
          <text y="-4">Buy</text>
          <text y="13" className="home-plan-graph-end-value">{formatCurrency(buyValues[maxYear] ?? 0, true)}</text>
        </g>
        <g className="home-plan-graph-end-label home-plan-graph-end-label--rent" transform={`translate(${x(maxYear) + 14} ${rentLabelY})`}>
          <text y="-4">Rent + invest</text>
          <text y="13" className="home-plan-graph-end-value">{formatCurrency(rentValues[maxYear] ?? 0, true)}</text>
        </g>

        {hoverYear !== null && (
          <g className="home-plan-graph-tooltip" transform={`translate(${tooltipX} ${tooltipY})`} filter="url(#home-plan-tooltip-shadow)">
            <rect width="206" height="78" rx="14" />
            <text x="16" y="20" className="home-plan-tooltip-year">Year {activeYear}</text>
            <circle cx="19" cy="40" r="4.5" className="home-plan-tooltip-buy" />
            <text x="32" y="43">Buy</text>
            <text x="190" y="43" className="home-plan-tooltip-value">{formatCurrency(buyValue, true)}</text>
            <circle cx="19" cy="61" r="4.5" className="home-plan-tooltip-rent" />
            <text x="32" y="64">Rent + invest</text>
            <text x="190" y="64" className="home-plan-tooltip-value">{formatCurrency(rentValue, true)}</text>
          </g>
        )}
      </svg>
    </div>
  );
}
