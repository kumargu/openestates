import { useMemo, useState, type ReactNode } from "react";
import { formatCurrency, type PlanInputs } from "./model.ts";
import type { RepaymentDashboardModel } from "./repaymentModel.ts";
import {
  buildRepaymentChartStories,
  type MonthlyRepaymentPoint,
  type RepaymentChartStories,
} from "./chartStories.ts";
import {
  ChartAnnotation,
  ChartHeading,
  ChartReadout,
  ReadoutValue,
  ScrubbableSvg,
} from "./charts/ChartPrimitives.tsx";
import {
  chartTickIndexes,
  linearScale,
  smoothLinePath,
  type ChartPoint,
} from "./charts/chartGeometry.ts";

const WIDTH = 900;
const PANE_WIDTH = 640;
const OVERVIEW_HEIGHT = 330;
const PANE_HEIGHT = 380;
const OVERVIEW_INSETS = { top: 28, right: 44, bottom: 42, left: 68 };
const PANE_INSETS = { top: 30, right: 28, bottom: 40, left: 62 };

type RepaymentDashboardProps = {
  inputs: PlanInputs;
  model: RepaymentDashboardModel;
  controls?: ReactNode;
  onStrategyChange?: (strategy: RepaymentDashboardModel["strategy"]) => void;
};

function durationLabel(months: number): string {
  const years = Math.floor(months / 12);
  const remainder = months % 12;
  if (years === 0) return `${remainder} mo`;
  if (remainder === 0) return `${years} yr`;
  return `${years} yr ${remainder} mo`;
}

function loanTimeLabel(paymentMonth: number): string {
  const years = Math.floor(paymentMonth / 12);
  const months = paymentMonth % 12;
  if (years === 0) return `Month ${Math.max(1, months)}`;
  if (months === 0) return `Year ${years}`;
  return `Year ${years}, month ${months}`;
}

function activeMonthlyPoint(
  points: MonthlyRepaymentPoint[],
  paymentMonth: number,
): MonthlyRepaymentPoint | undefined {
  if (points.length === 0) return undefined;
  return points[Math.max(0, Math.min(paymentMonth - 1, points.length - 1))];
}

function yearlyCheckpoints(points: MonthlyRepaymentPoint[]): MonthlyRepaymentPoint[] {
  return points.filter((point, index) => (
    point.paymentNumber % 12 === 0 || index === points.length - 1
  ));
}

function yearlyStarts(points: MonthlyRepaymentPoint[]): MonthlyRepaymentPoint[] {
  return points.filter((point) => (point.paymentNumber - 1) % 12 === 0);
}

function withPayoffPoint(
  points: MonthlyRepaymentPoint[],
  payoff: MonthlyRepaymentPoint | undefined,
): MonthlyRepaymentPoint[] {
  if (!payoff || points.at(-1)?.paymentNumber === payoff.paymentNumber) return points;
  return [...points, payoff];
}

function checkpointIndex(points: MonthlyRepaymentPoint[], paymentMonth: number): number {
  const index = points.findIndex((point) => point.paymentNumber >= paymentMonth);
  return index < 0 ? Math.max(0, points.length - 1) : index;
}

function monthlyPoints(
  points: MonthlyRepaymentPoint[],
  x: ReturnType<typeof linearScale>,
  y: ReturnType<typeof linearScale>,
  value: (point: MonthlyRepaymentPoint) => number,
): ChartPoint[] {
  return points.map((point) => ({
    x: x.map(point.paymentNumber),
    y: y.map(value(point)),
  }));
}

function YearTicks({
  paymentMonths,
  x,
  y,
}: {
  paymentMonths: number;
  x: ReturnType<typeof linearScale>;
  y: number;
}) {
  const years = Math.max(1, Math.ceil(paymentMonths / 12));
  return chartTickIndexes(years, 5).map((index) => {
    const year = index + 1;
    return (
      <text key={year} x={x.map(Math.min(paymentMonths, year * 12))} y={y} className="home-plan-chart-axis">
        {year}y
      </text>
    );
  });
}

function ValueGuides({
  values,
  x1,
  x2,
  y,
  format,
}: {
  values: number[];
  x1: number;
  x2: number;
  y: ReturnType<typeof linearScale>;
  format: (value: number) => string;
}) {
  return values.map((value) => (
    <g key={value} className="home-plan-value-guide" aria-hidden="true">
      <line x1={x1} x2={x2} y1={y.map(value)} y2={y.map(value)} />
      <text x={x1 - 9} y={y.map(value) + 4}>{format(value)}</text>
    </g>
  ));
}

function BalanceOverview({
  stories,
  model,
  activePaymentMonth,
  onPreviewPaymentMonth,
  onPinPaymentMonth,
}: {
  stories: RepaymentChartStories;
  model: RepaymentDashboardModel;
  activePaymentMonth: number;
  onPreviewPaymentMonth: (month: number | null) => void;
  onPinPaymentMonth: (month: number) => void;
}) {
  const baseline = stories.baselineMonthly;
  const selected = stories.selectedMonthly;
  const baselineYearly = yearlyCheckpoints(baseline);
  const selectedYearly = yearlyCheckpoints(selected);
  const horizon = Math.max(1, baseline.at(-1)?.paymentNumber ?? selected.at(-1)?.paymentNumber ?? 1);
  const maximumBalance = Math.max(
    1,
    ...baseline.map((point) => point.closingBalance),
    ...selected.map((point) => point.closingBalance),
  );
  const plotBottom = OVERVIEW_HEIGHT - OVERVIEW_INSETS.bottom;
  const x = linearScale([1, horizon], [OVERVIEW_INSETS.left, WIDTH - OVERVIEW_INSETS.right]);
  const y = linearScale([0, maximumBalance * 1.04], [plotBottom, OVERVIEW_INSETS.top]);
  const active = activeMonthlyPoint(selected, activePaymentMonth);
  const baselineActive = activeMonthlyPoint(baseline, activePaymentMonth);
  const selectedPayoff = selected.at(-1);
  const baselinePath = monthlyPoints(baselineYearly, x, y, (point) => point.closingBalance);
  const selectedPath = monthlyPoints(selectedYearly, x, y, (point) => point.closingBalance);
  const hasComparison = model.extraEmisPerYear > 0;

  return (
    <section className="home-plan-story home-plan-story--overview">
      <ChartHeading
        title={hasComparison
          ? `The selected path reaches zero in ${durationLabel(selectedPayoff?.paymentNumber ?? 0)}`
          : "How quickly does the loan disappear?"}
        conclusion={hasComparison
          ? `Without annual extras, the same loan runs for ${durationLabel(horizon)}.`
          : "Add annual extra EMIs to compare a faster path."}
      />
      <ScrubbableSvg
        width={WIDTH}
        height={OVERVIEW_HEIGHT}
        insets={OVERVIEW_INSETS}
        pointCount={selectedYearly.length}
        activeIndex={checkpointIndex(selectedYearly, activePaymentMonth)}
        label="Outstanding loan balance by payment month"
        className="home-plan-balance-overview"
        indexFromPoint={(clientX, _clientY, bounds) => {
          const svgX = (clientX - bounds.left) / Math.max(1, bounds.width) * WIDTH;
          return checkpointIndex(selectedYearly, Math.round(x.invert(svgX)));
        }}
        onPreviewIndex={(index) => onPreviewPaymentMonth(
          index == null ? null : selectedYearly[index].paymentNumber,
        )}
        onPinIndex={(index) => onPinPaymentMonth(selectedYearly[index].paymentNumber)}
      >
        <ValueGuides
          values={[0, maximumBalance / 2, maximumBalance]}
          x1={OVERVIEW_INSETS.left}
          x2={WIDTH - OVERVIEW_INSETS.right}
          y={y}
          format={(value) => formatCurrency(value, true)}
        />
        {hasComparison ? (
          <path d={smoothLinePath(baselinePath)} className="home-plan-curve is-baseline" />
        ) : null}
        <path d={smoothLinePath(selectedPath)} className="home-plan-curve is-selected" />
        <YearTicks paymentMonths={horizon} x={x} y={OVERVIEW_HEIGHT - 10} />
        {selectedPayoff ? (
          <g className="home-plan-payoff-marker" aria-hidden="true">
            <line
              x1={x.map(selectedPayoff.paymentNumber)}
              x2={x.map(selectedPayoff.paymentNumber)}
              y1={OVERVIEW_INSETS.top}
              y2={plotBottom}
            />
            <circle cx={x.map(selectedPayoff.paymentNumber)} cy={y.map(0)} r="4" />
            <text
              x={x.map(selectedPayoff.paymentNumber) - 7}
              y={OVERVIEW_INSETS.top + 12}
              textAnchor="end"
            >
              Paid off
            </text>
          </g>
        ) : null}
        {active ? (
          <g className="home-plan-chart-cursor">
            <line
              x1={x.map(active.paymentNumber)}
              x2={x.map(active.paymentNumber)}
              y1={OVERVIEW_INSETS.top}
              y2={plotBottom}
            />
            <circle cx={x.map(active.paymentNumber)} cy={y.map(active.closingBalance)} r="4" />
          </g>
        ) : null}
      </ScrubbableSvg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-selected">Selected path</span>
        {hasComparison ? <span className="is-baseline">Without extras</span> : null}
      </div>
      {active ? (
        <ChartReadout columns={3}>
          <ReadoutValue label="Point in loan" value={loanTimeLabel(active.paymentNumber)} />
          <ReadoutValue
            label={hasComparison ? "Selected balance" : "Balance"}
            value={formatCurrency(active.closingBalance, true)}
          />
          {hasComparison ? (
            <ReadoutValue
              label="Without extras"
              value={formatCurrency(baselineActive?.closingBalance ?? 0, true)}
            />
          ) : (
            <ReadoutValue label="Scheduled EMI" value={formatCurrency(active.scheduledEmi)} />
          )}
        </ChartReadout>
      ) : null}
    </section>
  );
}

function PaymentHandover({
  stories,
  activePaymentMonth,
  onPreviewPaymentMonth,
  onPinPaymentMonth,
}: {
  stories: RepaymentChartStories;
  activePaymentMonth: number;
  onPreviewPaymentMonth: (month: number | null) => void;
  onPinPaymentMonth: (month: number) => void;
}) {
  const points = stories.selectedMonthly.filter((point) => point.scheduledPayment > 0);
  const allShares = points.map((point) => ({
    point,
    interest: point.interestPaid / point.scheduledPayment * 100,
    principal: point.principalPaid / point.scheduledPayment * 100,
  }));
  const shares = allShares.filter(({ point }, index) => (
    (point.paymentNumber - 1) % 12 === 0 || index === allShares.length - 1
  ));
  const horizon = Math.max(1, points.at(-1)?.paymentNumber ?? 1);
  const plotBottom = PANE_HEIGHT - PANE_INSETS.bottom;
  const x = linearScale([1, horizon], [PANE_INSETS.left, PANE_WIDTH - PANE_INSETS.right]);
  const y = linearScale([0, 100], [plotBottom, PANE_INSETS.top]);
  const crossover = allShares.find(({ principal, interest }) => principal >= interest);
  const requestedActiveIndex = shares.findIndex(({ point }) => point.paymentNumber >= activePaymentMonth);
  const activeIndex = requestedActiveIndex < 0 ? Math.max(0, shares.length - 1) : requestedActiveIndex;
  const active = shares[activeIndex];
  const plotWidth = PANE_WIDTH - PANE_INSETS.left - PANE_INSETS.right;
  const barWidth = Math.min(24, plotWidth / Math.max(1, shares.length) * 0.58);

  return (
    <section className="home-plan-story">
      <ChartHeading
        title={crossover
          ? `Principal becomes the larger share in ${loanTimeLabel(crossover.point.paymentNumber).toLowerCase()}`
          : "Interest remains the larger share"}
        conclusion="Each column splits that year’s scheduled EMI between interest and principal."
      />
      <ScrubbableSvg
        width={PANE_WIDTH}
        height={PANE_HEIGHT}
        insets={PANE_INSETS}
        pointCount={shares.length}
        activeIndex={activeIndex}
        label="One hundred percent stacked columns showing annual interest and principal shares"
        className="home-plan-payment-handover"
        onPreviewIndex={(index) => onPreviewPaymentMonth(
          index == null ? null : shares[index].point.paymentNumber,
        )}
        onPinIndex={(index) => onPinPaymentMonth(shares[index].point.paymentNumber)}
      >
        <ValueGuides
          values={[0, 50, 100]}
          x1={PANE_INSETS.left}
          x2={PANE_WIDTH - PANE_INSETS.right}
          y={y}
          format={(value) => `${value}%`}
        />
        {shares.map(({ point, principal }, index) => {
          const columnX = x.map(point.paymentNumber) - barWidth / 2;
          const principalY = y.map(principal);
          return (
            <g
              key={point.paymentNumber}
              className={`home-plan-stack-bar ${index === activeIndex ? "is-active" : ""}`}
              aria-hidden="true"
            >
              <rect
                className="is-interest"
                x={columnX}
                y={PANE_INSETS.top}
                width={barWidth}
                height={Math.max(0, principalY - PANE_INSETS.top)}
              />
              <rect
                className="is-principal"
                x={columnX}
                y={principalY}
                width={barWidth}
                height={Math.max(0, plotBottom - principalY)}
              />
              <rect
                className="is-outline"
                x={columnX}
                y={PANE_INSETS.top}
                width={barWidth}
                height={plotBottom - PANE_INSETS.top}
              />
            </g>
          );
        })}
        {crossover ? (
          <ChartAnnotation
            x={x.map(crossover.point.paymentNumber)}
            top={PANE_INSETS.top}
            bottom={plotBottom}
            label="50 / 50"
          />
        ) : null}
        <YearTicks paymentMonths={horizon} x={x} y={PANE_HEIGHT - 10} />
      </ScrubbableSvg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-interest">Interest</span>
        <span className="is-principal">Principal</span>
      </div>
      {active ? (
        <ChartReadout columns={2}>
          <ReadoutValue label="Interest share" value={`${active.interest.toFixed(0)}%`} tone="interest" />
          <ReadoutValue label="Principal share" value={`${active.principal.toFixed(0)}%`} tone="principal" />
        </ChartReadout>
      ) : null}
    </section>
  );
}

function TimingCliff({
  model,
  activePaymentMonth,
  onPreviewPaymentMonth,
  onPinPaymentMonth,
}: {
  model: RepaymentDashboardModel;
  activePaymentMonth: number;
  onPreviewPaymentMonth: (month: number | null) => void;
  onPinPaymentMonth: (month: number) => void;
}) {
  const points = model.cadenceStartCurve;
  const activeYear = Math.max(1, Math.ceil(activePaymentMonth / 12));
  const activeIndex = Math.max(0, Math.min(activeYear - 1, points.length - 1));
  const active = points[activeIndex];
  const maximum = Math.max(1, ...points.map((point) => point.interestSaved));
  const plotBottom = PANE_HEIGHT - PANE_INSETS.bottom;
  const x = linearScale([1, Math.max(1, points.length)], [PANE_INSETS.left, PANE_WIDTH - PANE_INSETS.right]);
  const y = linearScale([0, maximum * 1.05], [plotBottom, PANE_INSETS.top]);
  const path = points.map((point) => ({ x: x.map(point.startYear), y: y.map(point.interestSaved) }));
  const halfIndex = model.markers.halfCadenceImpactStartYear == null
    ? -1
    : points.findIndex((point) => point.startYear === model.markers.halfCadenceImpactStartYear);
  const cadenceLabel = `${model.extraEmisPerYear} extra EMI${model.extraEmisPerYear === 1 ? "" : "s"}/year`;

  return (
    <section className="home-plan-story">
      <ChartHeading
        title={model.extraEmisPerYear === 0
          ? "Choose extra EMIs above to compare when they should start"
          : halfIndex < 0
            ? `Starting ${cadenceLabel} earlier saves more`
            : `Starting ${cadenceLabel} loses half its impact by year ${model.markers.halfCadenceImpactStartYear}`}
        conclusion="Each point starts the selected annual cadence in that year and keeps it going."
      />
      <ScrubbableSvg
        width={PANE_WIDTH}
        height={PANE_HEIGHT}
        insets={PANE_INSETS}
        pointCount={points.length}
        activeIndex={activeIndex}
        label={`Interest avoided by the start year of ${cadenceLabel}`}
        className="home-plan-timing-cliff"
        onPreviewIndex={(index) => onPreviewPaymentMonth(index == null ? null : (index + 1) * 12)}
        onPinIndex={(index) => onPinPaymentMonth((index + 1) * 12)}
      >
        <ValueGuides
          values={[0, maximum / 2, maximum]}
          x1={PANE_INSETS.left}
          x2={PANE_WIDTH - PANE_INSETS.right}
          y={y}
          format={(value) => formatCurrency(value, true)}
        />
        <path d={smoothLinePath(path)} className="home-plan-curve is-timing" />
        {halfIndex >= 0 ? (
          <ChartAnnotation
            x={x.map(points[halfIndex].startYear)}
            top={PANE_INSETS.top}
            bottom={plotBottom}
            label="Half impact"
          />
        ) : null}
        {chartTickIndexes(points.length).map((index) => (
          <text key={points[index].startYear} x={x.map(points[index].startYear)} y={PANE_HEIGHT - 10} className="home-plan-chart-axis">
            {points[index].startYear}y
          </text>
        ))}
        {active ? (
          <g className="home-plan-chart-cursor">
            <line
              x1={x.map(active.startYear)}
              x2={x.map(active.startYear)}
              y1={PANE_INSETS.top}
              y2={plotBottom}
            />
            <circle cx={x.map(active.startYear)} cy={y.map(active.interestSaved)} r="4" />
          </g>
        ) : null}
      </ScrubbableSvg>
      {active ? (
        <ChartReadout columns={3}>
          <ReadoutValue label="Cadence starts" value={`Year ${active.startYear}`} />
          <ReadoutValue label="Interest avoided" value={formatCurrency(active.interestSaved, true)} />
          <ReadoutValue
            label={model.strategy === "finish_earlier" ? "Loan ends earlier" : "Monthly EMI falls"}
            value={model.strategy === "finish_earlier"
              ? durationLabel(active.monthsSaved)
              : formatCurrency(active.monthlyEmiReduction)}
          />
        </ChartReadout>
      ) : null}
    </section>
  );
}

function StrategyPaths({
  stories,
  model,
  onSelect,
}: {
  stories: RepaymentChartStories;
  model: RepaymentDashboardModel;
  onSelect?: (strategy: RepaymentDashboardModel["strategy"]) => void;
}) {
  const finish = stories.finishEarlierMonthly;
  const lower = stories.lowerEmiMonthly;
  const finishPayoff = finish.at(-1);
  const lowerPayoff = lower.at(-1);
  const finishYearly = withPayoffPoint(yearlyStarts(finish), finishPayoff);
  const lowerYearly = withPayoffPoint(yearlyStarts(lower), lowerPayoff);
  const horizon = Math.max(1, finish.at(-1)?.paymentNumber ?? 1, lower.at(-1)?.paymentNumber ?? 1);
  const maximumEmi = Math.max(1, ...finish.map((point) => point.scheduledEmi), ...lower.map((point) => point.scheduledEmi));
  const plotBottom = OVERVIEW_HEIGHT - OVERVIEW_INSETS.bottom;
  const x = linearScale([1, horizon], [OVERVIEW_INSETS.left, WIDTH - OVERVIEW_INSETS.right]);
  const y = linearScale([0, maximumEmi * 1.05], [plotBottom, OVERVIEW_INSETS.top]);
  const finishPath = monthlyPoints(finishYearly, x, y, (point) => point.scheduledEmi);
  const lowerPath = monthlyPoints(lowerYearly, x, y, (point) => point.scheduledEmi);
  const lowerSummary = model.strategyComparison.find((point) => point.strategy === "lower_emi");

  return (
    <section className="home-plan-story home-plan-story--strategy">
      <ChartHeading
        title={model.extraEmisPerYear === 0
          ? "Add an annual extra EMI to compare both strategies"
          : "Same extra EMIs, two repayment outcomes"}
        conclusion={model.extraEmisPerYear === 0
          ? "Both strategies follow the same path without a prepayment."
          : "Choose whether each prepayment reduces tenure or recalculates the monthly EMI."}
      />
      {model.extraEmisPerYear > 0 ? (
        <div className="home-plan-strategy-toggle" role="group" aria-label="Prepayment strategy">
          <button
            type="button"
            className={model.strategy === "finish_earlier" ? "is-active" : undefined}
            aria-pressed={model.strategy === "finish_earlier"}
            onClick={() => onSelect?.("finish_earlier")}
          >
            Finish earlier
          </button>
          <button
            type="button"
            className={model.strategy === "lower_emi" ? "is-active" : undefined}
            aria-pressed={model.strategy === "lower_emi"}
            onClick={() => onSelect?.("lower_emi")}
          >
            Lower EMI
          </button>
        </div>
      ) : null}
      <svg
        viewBox={`0 0 ${WIDTH} ${OVERVIEW_HEIGHT}`}
        role="img"
        aria-label="Scheduled EMI under finish-earlier and lower-EMI strategies"
        className="home-plan-chart home-plan-strategy-paths"
      >
        <ValueGuides
          values={[0, maximumEmi / 2, maximumEmi]}
          x1={OVERVIEW_INSETS.left}
          x2={WIDTH - OVERVIEW_INSETS.right}
          y={y}
          format={(value) => formatCurrency(value)}
        />
        <path
          d={smoothLinePath(finishPath)}
          className={`home-plan-curve is-finish-earlier ${model.strategy === "finish_earlier" ? "is-selected" : ""}`}
        />
        {model.extraEmisPerYear > 0 ? (
          <path
            d={smoothLinePath(lowerPath)}
            className={`home-plan-curve is-lower-emi ${model.strategy === "lower_emi" ? "is-selected" : ""}`}
          />
        ) : null}
        <YearTicks paymentMonths={horizon} x={x} y={OVERVIEW_HEIGHT - 10} />
        {finishPayoff && model.extraEmisPerYear > 0 ? (
          <g className="home-plan-payoff-marker" aria-hidden="true">
            <line
              x1={x.map(finishPayoff.paymentNumber)}
              x2={x.map(finishPayoff.paymentNumber)}
              y1={OVERVIEW_INSETS.top}
              y2={plotBottom}
            />
            <circle
              cx={x.map(finishPayoff.paymentNumber)}
              cy={y.map(finishPayoff.scheduledEmi)}
              r="4"
            />
            <text
              x={x.map(finishPayoff.paymentNumber) - 7}
              y={OVERVIEW_INSETS.top + 12}
              textAnchor="end"
            >
              Paid off
            </text>
          </g>
        ) : null}
      </svg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-finish-earlier">Finish earlier</span>
        {model.extraEmisPerYear > 0 ? <span className="is-lower-emi">Lower EMI</span> : null}
      </div>
      <ChartReadout columns={2}>
        <ReadoutValue
          label="Finish earlier"
          value={finishPayoff ? `Paid off in ${durationLabel(finishPayoff.paymentNumber)}` : "Not repaid"}
        />
        <ReadoutValue
          label="Lower EMI"
          value={lowerPayoff
            ? `${formatCurrency(lowerSummary?.endingMonthlyEmi ?? lowerPayoff.scheduledEmi)} by payoff`
            : "Not repaid"}
        />
      </ChartReadout>
    </section>
  );
}

export function RepaymentDashboard({
  inputs,
  model,
  controls,
  onStrategyChange,
}: RepaymentDashboardProps) {
  const stories = useMemo(() => buildRepaymentChartStories(inputs, model), [inputs, model]);
  const [previewPaymentMonth, setPreviewPaymentMonth] = useState<number | null>(null);
  const [pinnedPaymentMonth, setPinnedPaymentMonth] = useState(12);
  const activePaymentMonth = previewPaymentMonth ?? pinnedPaymentMonth;

  if (stories.baselineMonthly.length === 0) {
    return (
      <div className="home-plan-mode-story">
        <header className="home-plan-mode-outcome">
          <h1>No loan is needed with this down payment.</h1>
        </header>
        {controls}
      </div>
    );
  }

  return (
    <div className="home-plan-mode-story">
      <header className="home-plan-mode-outcome">
        <h1>See how every extra EMI changes this loan.</h1>
      </header>
      {controls}

      <BalanceOverview
        stories={stories}
        model={model}
        activePaymentMonth={activePaymentMonth}
        onPreviewPaymentMonth={setPreviewPaymentMonth}
        onPinPaymentMonth={setPinnedPaymentMonth}
      />

      <section className="home-plan-chapter" aria-labelledby="home-plan-understand-loan">
        <header className="home-plan-chapter__heading">
          <p id="home-plan-understand-loan">Understand the loan</p>
        </header>
        <div className="home-plan-pane-grid">
          <PaymentHandover
            stories={stories}
            activePaymentMonth={activePaymentMonth}
            onPreviewPaymentMonth={setPreviewPaymentMonth}
            onPinPaymentMonth={setPinnedPaymentMonth}
          />
          <TimingCliff
            model={model}
            activePaymentMonth={activePaymentMonth}
            onPreviewPaymentMonth={setPreviewPaymentMonth}
            onPinPaymentMonth={setPinnedPaymentMonth}
          />
        </div>
      </section>

      <section className="home-plan-chapter" aria-labelledby="home-plan-choose-action">
        <header className="home-plan-chapter__heading">
          <p id="home-plan-choose-action">Choose what to do</p>
        </header>
        <StrategyPaths
          stories={stories}
          model={model}
          onSelect={onStrategyChange}
        />
      </section>
    </div>
  );
}
