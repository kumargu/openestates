import { useMemo, useState, type PointerEvent } from "react";
import { formatCurrency, type LoanJourney, type LoanJourneyPoint } from "./model.ts";

const GRAPH_WIDTH = 760;
const GRAPH_HEIGHT = 260;
const INSET = { left: 72, right: 28, top: 28, bottom: 40 };

function formatDuration(months: number): string {
  const years = Math.floor(months / 12);
  const remainingMonths = months % 12;
  if (years === 0) return `${remainingMonths}m`;
  return remainingMonths === 0 ? `${years}y` : `${years}y ${remainingMonths}m`;
}

function balancesByYear(points: LoanJourneyPoint[], maxYear: number): number[] {
  const byYear = new Map(points.map((point) => [point.year, point.balance]));
  return Array.from({ length: maxYear + 1 }, (_, year) => byYear.get(year) ?? 0);
}

function savingsGapPath(
  baselineBalances: number[],
  prepayBalances: number[],
  endYear: number,
  x: (year: number) => number,
  y: (balance: number) => number,
): string {
  const segments: string[] = [];
  for (let year = 0; year <= endYear; year += 1) {
    segments.push(`${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(baselineBalances[year] ?? 0).toFixed(1)}`);
  }
  for (let year = endYear; year >= 0; year -= 1) {
    segments.push(`L${x(year).toFixed(1)},${y(prepayBalances[year] ?? 0).toFixed(1)}`);
  }
  return `${segments.join(" ")} Z`;
}

function linePath(
  balances: number[],
  endYear: number,
  x: (year: number) => number,
  y: (balance: number) => number,
): string {
  return balances
    .slice(0, endYear + 1)
    .map((balance, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(balance).toFixed(1)}`)
    .join(" ");
}

type PayoffGraphProps = {
  journey: LoanJourney;
  baselineJourney: LoanJourney;
  extraEmisPerYear: number;
  selectedYear: number;
  onSelectYear: (year: number) => void;
};

export function PayoffGraph({
  journey,
  baselineJourney,
  extraEmisPerYear,
  selectedYear,
  onSelectYear,
}: PayoffGraphProps) {
  const [hoverYear, setHoverYear] = useState<number | null>(null);
  const showComparison = extraEmisPerYear > 0;

  const geometry = useMemo(() => {
    const maxYear = Math.max(
      journey.points.at(-1)?.year ?? 1,
      baselineJourney.points.at(-1)?.year ?? 1,
    );
    const prepayBalances = balancesByYear(journey.points, maxYear);
    const baselineBalances = balancesByYear(baselineJourney.points, maxYear);
    const maxBalance = Math.max(
      1,
      ...prepayBalances,
      ...baselineBalances,
    ) * 1.06;

    const plotWidth = GRAPH_WIDTH - INSET.left - INSET.right;
    const plotHeight = GRAPH_HEIGHT - INSET.top - INSET.bottom;
    const x = (year: number) => INSET.left + (year / maxYear) * plotWidth;
    const y = (balance: number) => INSET.top + plotHeight - (balance / maxBalance) * plotHeight;
    const floorY = INSET.top + plotHeight;

    const prepayLine = linePath(prepayBalances, maxYear, x, y);
    const baselineLine = linePath(baselineBalances, maxYear, x, y);
    const savingsGap = savingsGapPath(baselineBalances, prepayBalances, maxYear, x, y);

    const prepayFreeYear = Math.ceil(journey.loanFreeMonths / 12);
    const baselineFreeYear = Math.ceil(baselineJourney.loanFreeMonths / 12);

    const xTicks = [0, 5, 10, 15, 20].filter((year) => year <= maxYear);
    const yTicks = [0.25, 0.5, 0.75, 1].map((ratio) => ({
      ratio,
      value: maxBalance * ratio,
      y: y(maxBalance * ratio),
    }));

    return {
      maxYear,
      maxBalance,
      prepayBalances,
      baselineBalances,
      prepayLine,
      baselineLine,
      savingsGap,
      prepayFreeYear,
      baselineFreeYear,
      x,
      y,
      floorY,
      xTicks,
      yTicks,
    };
  }, [journey, baselineJourney]);

  const activeYear = Math.min(hoverYear ?? selectedYear, geometry.maxYear);
  const prepayBalance = geometry.prepayBalances[activeYear] ?? 0;
  const baselineBalance = geometry.baselineBalances[activeYear] ?? 0;
  const aheadBy = Math.max(0, baselineBalance - prepayBalance);
  const cursorX = geometry.x(activeYear);
  const prepayY = geometry.y(prepayBalance);
  const baselineY = geometry.y(baselineBalance);
  const tooltipX = cursorX > GRAPH_WIDTH - 230 ? cursorX - 214 : cursorX + 16;
  const tooltipY = Math.max(48, Math.min(GRAPH_HEIGHT - 118, Math.min(prepayY, baselineY) - 58));

  const updateHoverYear = (event: PointerEvent<SVGSVGElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    const svgX = ((event.clientX - bounds.left) / bounds.width) * GRAPH_WIDTH;
    const plotWidth = GRAPH_WIDTH - INSET.left - INSET.right;
    const nextYear = Math.round(((svgX - INSET.left) / plotWidth) * geometry.maxYear);
    setHoverYear(Math.max(0, Math.min(geometry.maxYear, nextYear)));
  };

  return (
    <div className="home-plan-payoff__chart-wrap">
      <div className="home-plan-payoff__chart-legend">
        <span className="home-plan-payoff__legend-item home-plan-payoff__legend-item--baseline">
          <i aria-hidden="true" />
          No extra EMIs
          <small>{formatCurrency(geometry.baselineBalances[activeYear] ?? 0, true)}</small>
        </span>
        <span className="home-plan-payoff__legend-item home-plan-payoff__legend-item--prepay">
          <i aria-hidden="true" />
          {showComparison ? `+${extraEmisPerYear} ${extraEmisPerYear === 1 ? "EMI" : "EMIs"}/yr` : "Current schedule"}
          <small>{formatCurrency(prepayBalance, true)}</small>
        </span>
        {showComparison && aheadBy > 0 && (
          <span className="home-plan-payoff__legend-gap">
            {hoverYear === null ? "Move across the chart" : `Year ${activeYear} · ${formatCurrency(aheadBy, true)} ahead`}
          </span>
        )}
        {!showComparison && (
          <span className="home-plan-payoff__legend-gap">Add extra EMIs to see how much faster you pay down</span>
        )}
      </div>

      <svg
        className="home-plan-payoff__chart"
        viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
        role="img"
        aria-label="Loan balance with and without extra EMIs"
        onPointerMove={updateHoverYear}
        onPointerLeave={() => setHoverYear(null)}
        onClick={() => onSelectYear(activeYear)}
      >
        <defs>
          <linearGradient id="payoff-savings-gap" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="var(--plan-monthly-deep)" stopOpacity="0.22" />
            <stop offset="100%" stopColor="var(--plan-monthly-deep)" stopOpacity="0.04" />
          </linearGradient>
          <filter id="payoff-tooltip-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="8" stdDeviation="10" floodColor="#1a1410" floodOpacity="0.1" />
          </filter>
        </defs>

        {geometry.yTicks.map(({ ratio, value, y: lineY }) => (
          <g key={ratio}>
            <line
              x1={INSET.left}
              x2={GRAPH_WIDTH - INSET.right}
              y1={lineY}
              y2={lineY}
              className="home-plan-gridline"
            />
            <text x={INSET.left - 12} y={lineY + 4} className="home-plan-axis-label home-plan-axis-label--y">
              {formatCurrency(value, true)}
            </text>
          </g>
        ))}

        {geometry.xTicks.map((year) => (
          <text
            key={year}
            x={geometry.x(year)}
            y={GRAPH_HEIGHT - 12}
            className="home-plan-axis-label home-plan-axis-label--x"
          >
            {year === 0 ? "Start" : `Y${year}`}
          </text>
        ))}

        {showComparison && (
          <path d={geometry.savingsGap} fill="url(#payoff-savings-gap)" className="home-plan-payoff__savings-gap" />
        )}

        {showComparison && (
          <path
            d={geometry.baselineLine}
            fill="none"
            className="home-plan-payoff__line home-plan-payoff__line--baseline"
          />
        )}

        <path
          d={geometry.prepayLine}
          fill="none"
          className="home-plan-payoff__line home-plan-payoff__line--prepay"
        />

        {showComparison && geometry.baselineFreeYear <= geometry.maxYear && (
          <g className="home-plan-payoff__milestone home-plan-payoff__milestone--baseline">
            <line
              x1={geometry.x(geometry.baselineFreeYear)}
              x2={geometry.x(geometry.baselineFreeYear)}
              y1={INSET.top}
              y2={geometry.floorY}
            />
            <text x={geometry.x(geometry.baselineFreeYear)} y={INSET.top - 8}>
              Original · {formatDuration(baselineJourney.loanFreeMonths)}
            </text>
          </g>
        )}

        {geometry.prepayFreeYear <= geometry.maxYear && (
          <g className="home-plan-payoff__milestone home-plan-payoff__milestone--prepay">
            <line
              x1={geometry.x(geometry.prepayFreeYear)}
              x2={geometry.x(geometry.prepayFreeYear)}
              y1={INSET.top}
              y2={geometry.floorY}
            />
            <text x={geometry.x(geometry.prepayFreeYear)} y={INSET.top - 8}>
              Loan-free · {formatDuration(journey.loanFreeMonths)}
            </text>
          </g>
        )}

        <g className="home-plan-payoff__cursor">
          <line x1={cursorX} x2={cursorX} y1={INSET.top} y2={geometry.floorY} />
          {showComparison && (
            <circle cx={cursorX} cy={baselineY} r="4.5" className="home-plan-payoff__dot home-plan-payoff__dot--baseline" />
          )}
          <circle cx={cursorX} cy={prepayY} r="5" className="home-plan-payoff__dot home-plan-payoff__dot--prepay" />
        </g>

        {(hoverYear !== null || selectedYear === activeYear) && (
          <g className="home-plan-payoff__tooltip" transform={`translate(${tooltipX}, ${tooltipY})`} filter="url(#payoff-tooltip-shadow)">
            <rect width="198" height={showComparison ? 88 : 56} rx="12" />
            <text x="14" y="22" className="home-plan-payoff__tooltip-year">
              {activeYear === 0 ? "Loan start" : `End of year ${activeYear}`}
            </text>
            {showComparison && (
              <text x="14" y="42" className="home-plan-payoff__tooltip-row home-plan-payoff__tooltip-row--baseline">
                No prepay · {formatCurrency(baselineBalance, true)}
              </text>
            )}
            <text x="14" y={showComparison ? 62 : 42} className="home-plan-payoff__tooltip-row home-plan-payoff__tooltip-row--prepay">
              {showComparison ? `+${extraEmisPerYear} EMIs · ` : "Balance · "}
              {formatCurrency(prepayBalance, true)}
            </text>
            {showComparison && aheadBy > 0 && (
              <text x="14" y="78" className="home-plan-payoff__tooltip-row home-plan-payoff__tooltip-row--saved">
                {formatCurrency(aheadBy, true)} ahead
              </text>
            )}
          </g>
        )}
      </svg>
    </div>
  );
}
