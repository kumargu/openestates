import { useState, type PointerEvent } from "react";
import type { PlanMilestone } from "./planFields.ts";
import { formatCurrency, type PlanProjection } from "./model.ts";

export type PlanScenarioId = "buy" | "rent";

type PlanGraphProps = {
  projection: PlanProjection;
  horizon: number;
  selected: PlanScenarioId;
  milestones: PlanMilestone[];
  hintedMilestoneYear: number | null;
  onHorizonChange: (year: number) => void;
  onPreviewYearChange: (year: number | null) => void;
  onMilestonePress: (milestone: PlanMilestone) => void;
};

type GraphSeries = {
  id: PlanScenarioId;
  label: string;
  values: number[];
};

type GapRegion = {
  id: string;
  winner: PlanScenarioId;
  path: string;
};

const GRAPH_WIDTH = 1080;
const GRAPH_HEIGHT = 420;
const GRAPH_INSET = { left: 72, right: 28, top: 36, bottom: 44 };

function graphSeries(projection: PlanProjection): GraphSeries[] {
  return [
    { id: "buy", label: "Buy this home", values: projection.points.map((point) => point.buyNetWorth) },
    { id: "rent", label: "Rent + invest", values: projection.points.map((point) => point.rentNetWorth) },
  ];
}

function gapRegions(
  buyValues: number[],
  rentValues: number[],
  endYear: number,
  x: (year: number) => number,
  y: (value: number) => number,
): GapRegion[] {
  const regions: GapRegion[] = [];
  const point = (year: number, value: number) => `${x(year).toFixed(1)},${y(value).toFixed(1)}`;

  for (let year = 0; year < endYear; year += 1) {
    const buyStart = buyValues[year] ?? 0;
    const buyEnd = buyValues[year + 1] ?? buyStart;
    const rentStart = rentValues[year] ?? 0;
    const rentEnd = rentValues[year + 1] ?? rentStart;
    const startGap = buyStart - rentStart;
    const endGap = buyEnd - rentEnd;

    if (startGap * endGap < 0) {
      const crossingRatio = startGap / (startGap - endGap);
      const crossingYear = year + crossingRatio;
      const crossingValue = buyStart + (buyEnd - buyStart) * crossingRatio;
      const crossing = point(crossingYear, crossingValue);
      regions.push({
        id: `${year}-before-crossing`,
        winner: startGap > 0 ? "buy" : "rent",
        path: `M${point(year, buyStart)} L${crossing} L${point(year, rentStart)} Z`,
      });
      regions.push({
        id: `${year}-after-crossing`,
        winner: endGap > 0 ? "buy" : "rent",
        path: `M${crossing} L${point(year + 1, buyEnd)} L${point(year + 1, rentEnd)} Z`,
      });
      continue;
    }

    if (startGap === 0 && endGap === 0) continue;
    regions.push({
      id: `${year}-gap`,
      winner: (startGap || endGap) > 0 ? "buy" : "rent",
      path: `M${point(year, buyStart)} L${point(year + 1, buyEnd)} L${point(year + 1, rentEnd)} L${point(year, rentStart)} Z`,
    });
  }

  return regions;
}

function milestoneShortLabel(label: string): string {
  if (label.startsWith("Buy")) return "H";
  if (label === "Break-even") return "=";
  if (label === "Loan free") return "✓";
  return "•";
}

export function PlanGraph({
  projection,
  horizon,
  selected,
  milestones,
  hintedMilestoneYear,
  onHorizonChange,
  onPreviewYearChange,
  onMilestonePress,
}: PlanGraphProps) {
  const [hoverYear, setHoverYear] = useState<number | null>(null);
  const series = graphSeries(projection);
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
  const selectedSeries = series.find((item) => item.id === selected) ?? series[0];
  const buyValues = series[0].values;
  const rentValues = series[1].values;
  const buyValue = buyValues[activeYear] ?? 0;
  const rentValue = rentValues[activeYear] ?? 0;
  const cursorX = x(activeYear);
  const buyWins = buyValue >= rentValue;
  const visibleGapRegions = gapRegions(buyValues, rentValues, activeYear, x, y);
  const tooltipX = cursorX > GRAPH_WIDTH - 240 ? cursorX - 220 : cursorX + 18;
  const tooltipY = Math.max(56, Math.min(GRAPH_HEIGHT - 124, y(selectedSeries.values[activeYear] ?? 0) - 50));
  const leadValue = Math.abs(buyValue - rentValue);
  const leadLabel = buyWins ? "Higher estimate: buy" : "Higher estimate: rent + invest";
  const graphMilestones = milestones.filter((milestone) => milestone.year <= maxYear);

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

        {visibleGapRegions.map((region) => (
          <path
            key={region.id}
            d={region.path}
            className={`home-plan-graph-gap home-plan-graph-gap--${region.winner}`}
          />
        ))}

        {series.map((item) => (
          <path
            key={item.id}
            d={line(item.values)}
            className={`home-plan-graph-line home-plan-graph-line--${item.id} ${selected === item.id ? "is-selected" : ""}`}
          />
        ))}

        {graphMilestones.map((milestone) => {
          const eventValue = selectedSeries.values[milestone.year] ?? 0;
          return (
            <g
              key={`${milestone.year}-${milestone.label}`}
              className={`home-plan-graph-event ${hintedMilestoneYear === milestone.year ? "is-hinted" : ""}`}
              transform={`translate(${x(milestone.year)} ${y(eventValue)})`}
              onClick={(event) => {
                event.stopPropagation();
                onMilestonePress(milestone);
              }}
            >
              <circle r="11" />
              <text y="3.5">{milestoneShortLabel(milestone.label)}</text>
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
