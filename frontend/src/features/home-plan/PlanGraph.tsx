import { useState, type KeyboardEvent } from "react";
import { formatCurrency, type PlanProjection } from "./model.ts";
import { buildWealthGapAreas, linePathForValues } from "./planGraphPaths.ts";

const WIDTH = 1080;
const HEIGHT = 420;
const INSETS = { top: 54, right: 160, bottom: 44, left: 72 };

type PlanGraphProps = {
  projection: PlanProjection;
  activeYear: number;
  onPreviewYearChange: (year: number | null) => void;
  onPinYear: (year: number) => void;
};

export function PlanGraph({
  projection,
  activeYear,
  onPreviewYearChange,
  onPinYear,
}: PlanGraphProps) {
  const [hoverYear, setHoverYear] = useState<number | null>(null);
  const points = projection.points;
  const maximumYear = points.length - 1;
  const boundedYear = Math.max(0, Math.min(hoverYear ?? activeYear, maximumYear));
  const active = points[boundedYear];
  const plotWidth = WIDTH - INSETS.left - INSETS.right;
  const plotHeight = HEIGHT - INSETS.top - INSETS.bottom;
  const values = points.flatMap((point) => [point.buyNetWorth, point.rentNetWorth]);
  const rawMinimum = Math.min(0, ...values);
  const rawMaximum = Math.max(1, ...values);
  const padding = Math.max(1, rawMaximum - rawMinimum) * 0.08;
  const minimumValue = rawMinimum < 0 ? rawMinimum - padding : 0;
  const maximumValue = rawMaximum + padding;
  const valueRange = maximumValue - minimumValue;
  const x = (year: number) => INSETS.left + year / Math.max(1, maximumYear) * plotWidth;
  const y = (value: number) => (
    INSETS.top + plotHeight - (value - minimumValue) / valueRange * plotHeight
  );
  const buyValues = points.map((point) => point.buyNetWorth);
  const rentValues = points.map((point) => point.rentNetWorth);
  const wealthGapAreas = buildWealthGapAreas(points, { x, y });
  const buyLeads = active.buyNetWorth >= active.rentNetWorth;
  const advantage = Math.abs(active.buyNetWorth - active.rentNetWorth);
  const cursorX = x(boundedYear);
  const tooltipX = cursorX > WIDTH - 260 ? cursorX - 226 : cursorX + 18;
  const tooltipY = Math.max(62, Math.min(
    HEIGHT - 120,
    Math.min(y(active.buyNetWorth), y(active.rentNetWorth)) - 34,
  ));
  const finalBuyY = y(buyValues[maximumYear] ?? 0);
  const finalRentY = y(rentValues[maximumYear] ?? 0);
  const labelsAreClose = Math.abs(finalBuyY - finalRentY) < 34;
  const labelMidpoint = (finalBuyY + finalRentY) / 2;
  const buyLabelY = labelsAreClose
    ? labelMidpoint + (finalBuyY >= finalRentY ? 24 : -24)
    : finalBuyY;
  const rentLabelY = labelsAreClose
    ? labelMidpoint + (finalRentY >= finalBuyY ? 24 : -24)
    : finalRentY;
  const loanFreeYear = projection.loanFreeYear;
  const showLoanFree = loanFreeYear != null && loanFreeYear > 0 && loanFreeYear <= maximumYear;

  function yearFromPointer(event: { clientX: number; currentTarget: SVGSVGElement }): number {
    const bounds = event.currentTarget.getBoundingClientRect();
    const svgX = (event.clientX - bounds.left) / Math.max(1, bounds.width) * WIDTH;
    const year = Math.round((svgX - INSETS.left) / plotWidth * maximumYear);
    return Math.max(0, Math.min(maximumYear, year));
  }

  function moveByKeyboard(event: KeyboardEvent<SVGSVGElement>) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? maximumYear
        : Math.max(0, Math.min(maximumYear, boundedYear + (event.key === "ArrowLeft" ? -1 : 1)));
    setHoverYear(null);
    onPreviewYearChange(null);
    onPinYear(next);
  }

  return (
    <section className="home-plan-rent-graph">
      <header className="home-plan-rent-graph__heading">
        <div>
          <h2>Projected net worth</h2>
          <p>After housing and investment costs.</p>
        </div>
        <strong>Year {boundedYear}: {buyLeads ? "buying" : "renting"} leads by {formatCurrency(advantage, true)}</strong>
      </header>
      <dl className="home-plan-rent-graph__mobile-values">
        <div className="is-buy"><dt>Buy</dt><dd>{formatCurrency(active.buyNetWorth, true)}</dd></div>
        <div className="is-rent"><dt>Rent</dt><dd>{formatCurrency(active.rentNetWorth, true)}</dd></div>
      </dl>
      <svg
        className="home-plan-rent-graph__svg"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="slider"
        tabIndex={0}
        aria-label={`Projected buy and rent wealth over ${maximumYear} years`}
        aria-valuemin={0}
        aria-valuemax={maximumYear}
        aria-valuenow={boundedYear}
        aria-valuetext={`Year ${boundedYear}, buy wealth ${formatCurrency(active.buyNetWorth, true)}, rent wealth ${formatCurrency(active.rentNetWorth, true)}`}
        onKeyDown={moveByKeyboard}
        onPointerMove={(event) => {
          const year = yearFromPointer(event);
          setHoverYear(year);
          onPreviewYearChange(year);
        }}
        onPointerLeave={() => {
          setHoverYear(null);
          onPreviewYearChange(null);
        }}
        onClick={(event) => onPinYear(yearFromPointer(event))}
      >
        <defs>
          <filter id="home-plan-rent-tooltip-shadow" x="-30%" y="-30%" width="160%" height="180%">
            <feDropShadow dx="0" dy="8" stdDeviation="10" floodColor="#1a1410" floodOpacity=".12" />
          </filter>
        </defs>
        {[0.25, 0.5, 0.75, 1].map((ratio) => {
          const value = minimumValue + valueRange * ratio;
          return (
            <g key={ratio} className="home-plan-rent-graph__guide" aria-hidden="true">
              <line x1={INSETS.left} x2={WIDTH - INSETS.right} y1={y(value)} y2={y(value)} />
              <text x={INSETS.left - 12} y={y(value) + 4}>{formatCurrency(value, true)}</text>
            </g>
          );
        })}
        {wealthGapAreas.map((area, index) => (
          <path
            key={`${area.leader}-${index}`}
            d={area.path}
            className={`home-plan-rent-graph__gap is-${area.leader}`}
            aria-hidden="true"
          />
        ))}
        {showLoanFree ? (
          <g className="home-plan-rent-graph__loan-free" aria-hidden="true">
            <line x1={x(loanFreeYear)} x2={x(loanFreeYear)} y1={INSETS.top} y2={HEIGHT - INSETS.bottom} />
            <text x={x(loanFreeYear)} y={HEIGHT - INSETS.bottom + 20}>Loan-free</text>
          </g>
        ) : null}
        <path d={linePathForValues(buyValues, x, y)} className="home-plan-rent-graph__line is-buy" />
        <path d={linePathForValues(rentValues, x, y)} className="home-plan-rent-graph__line is-rent" />
        {[0, 5, 10, 15, 20].filter((year) => year <= maximumYear).map((year) => (
          <text key={year} x={x(year)} y={HEIGHT - 10} className="home-plan-rent-graph__axis">
            {year === 0 ? "Now" : `${year}y`}
          </text>
        ))}
        <g className="home-plan-rent-graph__cursor">
          <line x1={cursorX} x2={cursorX} y1={INSETS.top} y2={HEIGHT - INSETS.bottom} />
          <circle cx={cursorX} cy={y(active.buyNetWorth)} r="5" className="is-buy" />
          <circle cx={cursorX} cy={y(active.rentNetWorth)} r="5" className="is-rent" />
        </g>
        <g className="home-plan-rent-graph__end is-buy" transform={`translate(${x(maximumYear) + 14} ${buyLabelY})`}>
          <text>Buy</text>
          <text y="17">{formatCurrency(buyValues[maximumYear] ?? 0, true)}</text>
        </g>
        <g className="home-plan-rent-graph__end is-rent" transform={`translate(${x(maximumYear) + 14} ${rentLabelY})`}>
          <text>Rent</text>
          <text y="17">{formatCurrency(rentValues[maximumYear] ?? 0, true)}</text>
        </g>
        {hoverYear != null ? (
          <g
            className="home-plan-rent-graph__tooltip"
            transform={`translate(${tooltipX} ${tooltipY})`}
            filter="url(#home-plan-rent-tooltip-shadow)"
          >
            <rect width="208" height="96" rx="10" />
            <text x="16" y="21" className="is-year">Year {boundedYear}</text>
            <circle cx="19" cy="42" r="4" className="is-buy" />
            <text x="32" y="45">Buy</text>
            <text x="192" y="45" textAnchor="end">{formatCurrency(active.buyNetWorth, true)}</text>
            <circle cx="19" cy="64" r="4" className="is-rent" />
            <text x="32" y="67">Rent</text>
            <text x="192" y="67" textAnchor="end">{formatCurrency(active.rentNetWorth, true)}</text>
            <text x="16" y="87" className="is-lead">{buyLeads ? "Buy ahead" : "Rent ahead"}</text>
            <text x="192" y="87" textAnchor="end">{formatCurrency(advantage, true)}</text>
          </g>
        ) : null}
      </svg>
    </section>
  );
}
