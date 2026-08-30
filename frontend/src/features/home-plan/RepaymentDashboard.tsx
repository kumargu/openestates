import { useState, type PointerEvent } from "react";
import { formatCurrency } from "./model.ts";
import type {
  PrepaymentRunPoint,
  RepaymentDashboardModel,
  RepaymentYearPoint,
} from "./repaymentModel.ts";
import type { RepaymentStrategy } from "./financeEngine.ts";

const CHART_WIDTH = 760;
const MIX_CHART_HEIGHT = 280;
const MINI_CHART_HEIGHT = 180;
const CHART_INSET = { left: 48, right: 18, top: 24, bottom: 30 };

type RepaymentDashboardProps = {
  model: RepaymentDashboardModel;
  onStrategyChange: (strategy: RepaymentStrategy) => void;
};

function durationLabel(months: number): string {
  const years = Math.floor(months / 12);
  const remainingMonths = months % 12;
  if (years === 0) return `${remainingMonths} mo`;
  if (remainingMonths === 0) return `${years} yr`;
  return `${years} yr ${remainingMonths} mo`;
}

function outcomeFor(model: RepaymentDashboardModel): string | null {
  if (model.extraEmisPerYear === 0) return null;
  if (model.strategy === "lower_emi") {
    return `After year 1, EMI becomes ${formatCurrency(model.firstRecalculatedMonthlyEmi)} · ${formatCurrency(model.interestSaved, true)} lifetime interest saved`;
  }
  return `Loan-free ${durationLabel(model.monthsSaved)} earlier · ${formatCurrency(model.interestSaved, true)} less interest`;
}

function yearFromPointer(
  event: PointerEvent<SVGSVGElement>,
  pointCount: number,
): number {
  const bounds = event.currentTarget.getBoundingClientRect();
  const svgX = ((event.clientX - bounds.left) / bounds.width) * CHART_WIDTH;
  const plotWidth = CHART_WIDTH - CHART_INSET.left - CHART_INSET.right;
  const ratio = (svgX - CHART_INSET.left) / plotWidth;
  return Math.max(0, Math.min(pointCount - 1, Math.round(ratio * (pointCount - 1))));
}

function chartTicks(length: number): number[] {
  if (length <= 1) return [0];
  const last = length - 1;
  const step = last <= 10 ? 2 : 5;
  const ticks = [0];
  for (let index = step; index < last; index += step) ticks.push(index);
  if (ticks.at(-1) !== last) ticks.push(last);
  return ticks;
}

function RepaymentMixChart({
  points,
  crossoverYear,
}: {
  points: RepaymentYearPoint[];
  crossoverYear: number | null;
}) {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const displayIndex = Math.min(hoverIndex ?? 0, Math.max(0, points.length - 1));
  const display = points[displayIndex];
  const plotWidth = CHART_WIDTH - CHART_INSET.left - CHART_INSET.right;
  const plotHeight = MIX_CHART_HEIGHT - CHART_INSET.top - CHART_INSET.bottom;
  const paymentMax = Math.max(1, ...points.map((point) => (
    point.interestPaid + point.principalPaid + point.extraPaid
  )));
  const balanceMax = Math.max(1, ...points.map((point) => point.balance));
  const step = points.length > 0 ? plotWidth / points.length : plotWidth;
  const barWidth = Math.max(3, Math.min(24, step * 0.62));
  const x = (index: number) => CHART_INSET.left + step * index + step / 2;
  const paymentHeight = (value: number) => value / paymentMax * plotHeight;
  const balanceY = (value: number) => CHART_INSET.top + plotHeight - value / balanceMax * plotHeight;
  const balancePath = points.map((point, index) => (
    `${index === 0 ? "M" : "L"} ${x(index)} ${balanceY(point.balance)}`
  )).join(" ");
  const crossoverIndex = crossoverYear == null
    ? null
    : points.findIndex((point) => point.year === crossoverYear);

  return (
    <div className="home-plan-repayment__panel home-plan-repayment__panel--wide">
      <div className="home-plan-repayment__panel-heading">
        <h3>Where each year’s EMI goes</h3>
        <div className="home-plan-repayment__legend" aria-hidden="true">
          <span className="is-interest">Interest</span>
          <span className="is-principal">Principal</span>
          <span className="is-extra">Extra</span>
          <span className="is-balance">Balance</span>
        </div>
      </div>
      <svg
        className="home-plan-repayment__mix-chart"
        viewBox={`0 0 ${CHART_WIDTH} ${MIX_CHART_HEIGHT}`}
        role="img"
        aria-label="Annual interest, principal, extra payments and remaining loan balance"
        onPointerMove={(event) => setHoverIndex(yearFromPointer(event, points.length))}
        onPointerLeave={() => setHoverIndex(null)}
      >
        {[0.25, 0.5, 0.75, 1].map((ratio) => {
          const y = CHART_INSET.top + plotHeight * (1 - ratio);
          return <line key={ratio} x1={CHART_INSET.left} x2={CHART_WIDTH - CHART_INSET.right} y1={y} y2={y} className="home-plan-repayment__grid" />;
        })}
        {points.map((point, index) => {
          const interestHeight = paymentHeight(point.interestPaid);
          const principalHeight = paymentHeight(point.principalPaid);
          const extraHeight = paymentHeight(point.extraPaid);
          const baseY = CHART_INSET.top + plotHeight;
          return (
            <g key={point.year} className={index === displayIndex ? "is-focused" : undefined}>
              <rect x={x(index) - barWidth / 2} y={baseY - interestHeight} width={barWidth} height={interestHeight} className="home-plan-repayment__bar-interest" />
              <rect x={x(index) - barWidth / 2} y={baseY - interestHeight - principalHeight} width={barWidth} height={principalHeight} className="home-plan-repayment__bar-principal" />
              <rect x={x(index) - barWidth / 2} y={baseY - interestHeight - principalHeight - extraHeight} width={barWidth} height={extraHeight} className="home-plan-repayment__bar-extra" />
            </g>
          );
        })}
        <path d={balancePath} className="home-plan-repayment__balance-line" />
        {crossoverIndex != null && crossoverIndex >= 0 && (
          <g className="home-plan-repayment__marker">
            <line x1={x(crossoverIndex)} x2={x(crossoverIndex)} y1={CHART_INSET.top} y2={CHART_INSET.top + plotHeight} />
            <text x={x(crossoverIndex)} y={CHART_INSET.top + 10}>Principal leads</text>
          </g>
        )}
        {chartTicks(points.length).map((index) => (
          <text key={index} x={x(index)} y={MIX_CHART_HEIGHT - 8} className="home-plan-repayment__axis-label">
            {points[index]?.year}y
          </text>
        ))}
        {display && (
          <g className="home-plan-repayment__cursor">
            <line x1={x(displayIndex)} x2={x(displayIndex)} y1={CHART_INSET.top} y2={CHART_INSET.top + plotHeight} />
            <circle cx={x(displayIndex)} cy={balanceY(display.balance)} r="4" />
          </g>
        )}
      </svg>
      {display && (
        <dl className="home-plan-repayment__readout" aria-live="polite">
          <div><dt>Year</dt><dd>{display.year}</dd></div>
          <div><dt>Interest</dt><dd>{formatCurrency(display.interestPaid, true)}</dd></div>
          <div><dt>Principal</dt><dd>{formatCurrency(display.principalPaid, true)}</dd></div>
          <div><dt>Extra</dt><dd>{formatCurrency(display.extraPaid, true)}</dd></div>
          <div><dt>Balance</dt><dd>{formatCurrency(display.balance, true)}</dd></div>
        </dl>
      )}
    </div>
  );
}

function linePath(values: number[], x: (index: number) => number, y: (value: number) => number): string {
  return values.map((value, index) => `${index === 0 ? "M" : "L"} ${x(index)} ${y(value)}`).join(" ");
}

function ImpactChart({
  label,
  points,
  values,
  focusIndex,
  markerYear,
  onFocus,
}: {
  label: string;
  points: PrepaymentRunPoint[];
  values: number[];
  focusIndex: number;
  markerYear?: number | null;
  onFocus: (index: number | null) => void;
}) {
  const plotWidth = CHART_WIDTH - CHART_INSET.left - CHART_INSET.right;
  const plotHeight = MINI_CHART_HEIGHT - CHART_INSET.top - CHART_INSET.bottom;
  const maximum = Math.max(1, ...values);
  const x = (index: number) => CHART_INSET.left + (points.length <= 1 ? 0 : index / (points.length - 1) * plotWidth);
  const y = (value: number) => CHART_INSET.top + plotHeight - value / maximum * plotHeight;
  const markerIndex = markerYear == null
    ? null
    : points.findIndex((point) => point.throughYear === markerYear);

  return (
    <div className="home-plan-repayment__panel">
      <h3>{label}</h3>
      <svg
        className="home-plan-repayment__impact-chart"
        viewBox={`0 0 ${CHART_WIDTH} ${MINI_CHART_HEIGHT}`}
        role="img"
        aria-label={`${label} by number of years making extra payments`}
        onPointerMove={(event) => onFocus(yearFromPointer(event, points.length))}
        onPointerLeave={() => onFocus(null)}
      >
        {[0.5, 1].map((ratio) => {
          const gridY = y(maximum * ratio);
          return <line key={ratio} x1={CHART_INSET.left} x2={CHART_WIDTH - CHART_INSET.right} y1={gridY} y2={gridY} className="home-plan-repayment__grid" />;
        })}
        <path d={`${linePath(values, x, y)} L ${x(values.length - 1)} ${CHART_INSET.top + plotHeight} L ${x(0)} ${CHART_INSET.top + plotHeight} Z`} className="home-plan-repayment__impact-area" />
        <path d={linePath(values, x, y)} className="home-plan-repayment__impact-line" />
        {markerIndex != null && markerIndex >= 0 && (
          <g className="home-plan-repayment__marker">
            <line x1={x(markerIndex)} x2={x(markerIndex)} y1={CHART_INSET.top} y2={CHART_INSET.top + plotHeight} />
            <text x={x(markerIndex)} y={CHART_INSET.top + 10}>½ first-year impact</text>
          </g>
        )}
        {chartTicks(points.length).map((index) => (
          <text key={index} x={x(index)} y={MINI_CHART_HEIGHT - 7} className="home-plan-repayment__axis-label">
            {points[index]?.throughYear}y
          </text>
        ))}
        <g className="home-plan-repayment__cursor">
          <line x1={x(focusIndex)} x2={x(focusIndex)} y1={CHART_INSET.top} y2={CHART_INSET.top + plotHeight} />
          <circle cx={x(focusIndex)} cy={y(values[focusIndex] ?? 0)} r="4" />
        </g>
      </svg>
    </div>
  );
}

function PrepaymentImpact({ model }: { model: RepaymentDashboardModel }) {
  const points = model.prepaymentRun;
  const defaultIndex = Math.min(5, Math.max(0, points.length - 1));
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const focusIndex = Math.min(hoverIndex ?? defaultIndex, Math.max(0, points.length - 1));
  const focus = points[focusIndex];

  if (model.extraEmisPerYear === 0 || !focus) {
    return <p className="home-plan-repayment__empty">Choose an extra-EMI rhythm to see its impact over time.</p>;
  }

  return (
    <>
      <div className="home-plan-repayment__impact-grid">
        <ImpactChart
          label="Interest avoided"
          points={points}
          values={points.map((point) => point.interestSaved)}
          focusIndex={focusIndex}
          onFocus={setHoverIndex}
        />
        <ImpactChart
          label="What one more year adds"
          points={points}
          values={points.map((point) => point.incrementalInterestSaved)}
          focusIndex={focusIndex}
          markerYear={model.halfImpactYear}
          onFocus={setHoverIndex}
        />
      </div>
      <p className="home-plan-repayment__impact-readout" aria-live="polite">
        Extra payments through year {focus.throughYear}: <strong>{formatCurrency(focus.interestSaved, true)} less interest</strong>
        {model.strategy === "finish_earlier"
          ? ` · ${durationLabel(focus.monthsSaved)} earlier`
          : ` · ${formatCurrency(focus.monthlyEmiReduction)} lower EMI`}
      </p>
    </>
  );
}

export function RepaymentDashboard({ model, onStrategyChange }: RepaymentDashboardProps) {
  const outcome = outcomeFor(model);
  return (
    <section className="home-plan-repayment" aria-labelledby="home-plan-repayment-title">
      <header className="home-plan-repayment__header">
        <div>
          <h2 id="home-plan-repayment-title">Repayment</h2>
          {outcome ? <p>{outcome}</p> : null}
        </div>
        {model.extraEmisPerYear > 0 ? <div className="home-plan-repayment__strategy" role="group" aria-label="What extra payments buy">
          <span>Extra payments buy</span>
          <div>
            <button
              type="button"
              className={model.strategy === "finish_earlier" ? "is-active" : undefined}
              aria-pressed={model.strategy === "finish_earlier"}
              onClick={() => onStrategyChange("finish_earlier")}
            >
              Time
            </button>
            <button
              type="button"
              className={model.strategy === "lower_emi" ? "is-active" : undefined}
              aria-pressed={model.strategy === "lower_emi"}
              onClick={() => onStrategyChange("lower_emi")}
            >
              Monthly room
            </button>
          </div>
        </div> : null}
      </header>

      <RepaymentMixChart points={model.repaymentYears} crossoverYear={model.crossoverYear} />
      <PrepaymentImpact model={model} />

      <details className="home-plan-repayment__formula">
        <summary>How the calculation works</summary>
        <div>
          <p>Interest is calculated monthly on the outstanding balance. Extra payments use today’s EMI and go directly to principal once a year.</p>
          <p><code>B</code> is the balance, <code>r</code> the monthly rate and <code>n</code> the remaining months.</p>
          <code>EMI = B × r × (1 + r)ⁿ ÷ ((1 + r)ⁿ − 1)</code>
          <code>Months = −ln(1 − B × r ÷ EMI) ÷ ln(1 + r)</code>
        </div>
      </details>
    </section>
  );
}
