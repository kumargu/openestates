import { useMemo, useState } from "react";
import { formatCurrency, type PlanInputs } from "./model.ts";
import type {
  OneOffExtraPaymentPoint,
  RepaymentDashboardModel,
  RepaymentYearPoint,
} from "./repaymentModel.ts";
import {
  buildRepaymentChartStories,
  type MonthlyRepaymentPoint,
  type RepaymentChartStories,
} from "./chartStories.ts";
import {
  ChartHeading,
  ChartReadout,
  ReadoutValue,
  ScrubbableSvg,
} from "./charts/ChartPrimitives.tsx";
import {
  bandPath,
  chartTickIndexes,
  linearScale,
  smoothLinePath,
  type ChartPoint,
} from "./charts/chartGeometry.ts";

const WIDTH = 900;
const OVERVIEW_HEIGHT = 360;
const FRAME_HEIGHT = 330;
const OVERVIEW_INSETS = { top: 42, right: 76, bottom: 44, left: 70 };
const FRAME_INSETS = { top: 40, right: 42, bottom: 44, left: 70 };

type RepaymentDashboardProps = {
  inputs: PlanInputs;
  model: RepaymentDashboardModel;
  onStrategyChange: (strategy: RepaymentDashboardModel["strategy"]) => void;
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

function yearlyCheckpoints(points: MonthlyRepaymentPoint[]): MonthlyRepaymentPoint[] {
  return points.filter((point, index) => (
    point.paymentNumber % 12 === 0 || index === points.length - 1
  ));
}

function checkpointIndex(points: MonthlyRepaymentPoint[], paymentMonth: number): number {
  const index = points.findIndex((point) => point.paymentNumber >= paymentMonth);
  return index < 0 ? Math.max(0, points.length - 1) : index;
}

function activeMonthlyPoint(
  points: MonthlyRepaymentPoint[],
  paymentMonth: number,
): MonthlyRepaymentPoint | undefined {
  if (points.length === 0) return undefined;
  return points[Math.max(0, Math.min(paymentMonth - 1, points.length - 1))];
}

function balanceAt(points: MonthlyRepaymentPoint[], paymentMonth: number): number {
  if (paymentMonth > (points.at(-1)?.paymentNumber ?? 0)) return 0;
  return activeMonthlyPoint(points, paymentMonth)?.closingBalance ?? 0;
}

function outcomeSentence(model: RepaymentDashboardModel): string {
  if (model.extraEmisPerYear === 0) {
    return `At ${formatCurrency(model.openingMonthlyEmi)} per month, this loan follows its original payoff path.`;
  }
  if (model.strategy === "finish_earlier") {
    return `${model.extraEmisPerYear} extra EMI${model.extraEmisPerYear === 1 ? "" : "s"}/year makes you debt-free ${durationLabel(model.monthsSaved)} earlier and avoids ${formatCurrency(model.interestSaved, true)} of interest.`;
  }
  return `After the first annual prepayment, EMI falls from ${formatCurrency(model.openingMonthlyEmi)} to ${formatCurrency(model.firstRecalculatedMonthlyEmi)} and avoids ${formatCurrency(model.interestSaved, true)} of interest.`;
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
  const baselinePayoff = baseline.at(-1);
  const selectedPayoff = selected.at(-1);
  const horizon = Math.max(
    1,
    baselinePayoff?.paymentNumber ?? 1,
    selectedPayoff?.paymentNumber ?? 1,
  );
  const checkpointMonths = [...new Set([
    1,
    ...Array.from({ length: Math.ceil(horizon / 12) }, (_, index) => Math.min(horizon, (index + 1) * 12)),
    baselinePayoff?.paymentNumber ?? horizon,
    selectedPayoff?.paymentNumber ?? horizon,
  ])].sort((left, right) => left - right);
  const maximumBalance = Math.max(
    1,
    ...baseline.map((point) => point.closingBalance),
    ...selected.map((point) => point.closingBalance),
  );
  const plotBottom = OVERVIEW_HEIGHT - OVERVIEW_INSETS.bottom;
  const x = linearScale([1, horizon], [OVERVIEW_INSETS.left, WIDTH - OVERVIEW_INSETS.right]);
  const y = linearScale([0, maximumBalance * 1.04], [plotBottom, OVERVIEW_INSETS.top]);
  const baselinePath = checkpointMonths.map((month) => ({
    x: x.map(month),
    y: y.map(balanceAt(baseline, month)),
  }));
  const selectedPath = checkpointMonths.map((month) => ({
    x: x.map(month),
    y: y.map(balanceAt(selected, month)),
  }));
  const selectedLinePath = selectedPath.filter((_, index) => (
    checkpointMonths[index] <= (selectedPayoff?.paymentNumber ?? horizon)
  ));
  const selectedYearly = yearlyCheckpoints(selected);
  const activeIndex = checkpointIndex(selectedYearly, activePaymentMonth);
  const active = selectedYearly[activeIndex];
  const baselineActive = active ? balanceAt(baseline, active.paymentNumber) : 0;
  const gaps = checkpointMonths.map((month) => ({
    month,
    gap: balanceAt(baseline, month) - balanceAt(selected, month),
  }));
  const largestGap = gaps.reduce((largest, point) => (
    point.gap > largest.gap ? point : largest
  ), gaps[0] ?? { month: 1, gap: 0 });
  const hasComparison = model.extraEmisPerYear > 0;
  const samePayoff = baselinePayoff?.paymentNumber === selectedPayoff?.paymentNumber;

  return (
    <section className="home-plan-frame home-plan-frame--outcome">
      <ChartHeading
        title={outcomeSentence(model)}
        conclusion={hasComparison
          ? "Compare the selected plan with the same loan without annual prepayments."
          : "Add annual extra EMIs above to compare a faster or lighter repayment path."}
      />
      <ScrubbableSvg
        width={WIDTH}
        height={OVERVIEW_HEIGHT}
        insets={OVERVIEW_INSETS}
        pointCount={selectedYearly.length}
        activeIndex={activeIndex}
        label="Selected and original outstanding loan balance by year"
        valueText={active
          ? `${loanTimeLabel(active.paymentNumber)}, selected balance ${formatCurrency(active.closingBalance, true)}, without extras ${formatCurrency(baselineActive, true)}`
          : undefined}
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
        {[0, maximumBalance / 2, maximumBalance].map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={OVERVIEW_INSETS.left}
              x2={WIDTH - OVERVIEW_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={OVERVIEW_INSETS.left - 9} y={y.map(value) + 4}>
              {formatCurrency(value, true)}
            </text>
          </g>
        ))}
        {hasComparison ? (
          <path
            d={bandPath(selectedPath, baselinePath)}
            className="home-plan-balance-gap"
          />
        ) : null}
        <path d={smoothLinePath(baselinePath)} className="home-plan-curve is-baseline" />
        {hasComparison ? (
          <path d={smoothLinePath(selectedLinePath)} className="home-plan-curve is-selected" />
        ) : null}
        {hasComparison && largestGap.gap > 0 ? (
          <text
            x={x.map(largestGap.month)}
            y={(y.map(balanceAt(baseline, largestGap.month)) + y.map(balanceAt(selected, largestGap.month))) / 2}
            className="home-plan-gap-label"
          >
            Balance reduced
          </text>
        ) : null}
        {chartTickIndexes(Math.ceil(horizon / 12) + 1, 5).map((index) => {
          const month = Math.min(horizon, index * 12 || 1);
          return (
            <text key={index} x={x.map(month)} y={OVERVIEW_HEIGHT - 11} className="home-plan-chart-axis">
              {index === 0 ? "Now" : `${index}y`}
            </text>
          );
        })}
        {baselinePayoff ? (
          <g className="home-plan-payoff-marker is-baseline-payoff" aria-hidden="true">
            <line
              x1={x.map(baselinePayoff.paymentNumber)}
              x2={x.map(baselinePayoff.paymentNumber)}
              y1={OVERVIEW_INSETS.top}
              y2={plotBottom}
            />
            <text
              x={x.map(baselinePayoff.paymentNumber) - 5}
              y={plotBottom - 9}
              textAnchor="end"
            >
              {samePayoff ? "Both plans" : "Without extras"} · {durationLabel(baselinePayoff.paymentNumber)}
            </text>
          </g>
        ) : null}
        {hasComparison && selectedPayoff && !samePayoff ? (
          <g className="home-plan-payoff-marker is-selected-payoff" aria-hidden="true">
            <line
              x1={x.map(selectedPayoff.paymentNumber)}
              x2={x.map(selectedPayoff.paymentNumber)}
              y1={OVERVIEW_INSETS.top}
              y2={plotBottom}
            />
            <circle cx={x.map(selectedPayoff.paymentNumber)} cy={y.map(0)} r="4" />
            <text
              x={x.map(selectedPayoff.paymentNumber) - 5}
              y={plotBottom - 26}
              textAnchor="end"
            >
              Selected plan · {durationLabel(selectedPayoff.paymentNumber)}
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
            <circle cx={x.map(active.paymentNumber)} cy={y.map(active.closingBalance)} r="5" />
            <text
              x={x.map(active.paymentNumber) + (x.map(active.paymentNumber) > WIDTH - 190 ? -10 : 10)}
              y={Math.max(OVERVIEW_INSETS.top + 12, y.map(active.closingBalance) - 10)}
              textAnchor={x.map(active.paymentNumber) > WIDTH - 190 ? "end" : "start"}
              className="home-plan-point-value"
            >
              {formatCurrency(active.closingBalance, true)}
            </text>
          </g>
        ) : null}
      </ScrubbableSvg>
      {active ? (
        <ChartReadout columns={3}>
          <ReadoutValue label="Point in loan" value={loanTimeLabel(active.paymentNumber)} />
          <ReadoutValue label="Selected balance" value={formatCurrency(active.closingBalance, true)} />
          <ReadoutValue label="Without extras" value={formatCurrency(baselineActive, true)} />
        </ChartReadout>
      ) : null}
    </section>
  );
}

function annualTotal(point: RepaymentYearPoint): number {
  return point.interestPaid + point.principalPaid + point.extraPaid;
}

function PaymentMechanics({
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
  const points = model.recurrentSchedule.filter((point) => annualTotal(point) > 0);
  const activeYear = Math.max(1, Math.ceil(activePaymentMonth / 12));
  const requestedActiveIndex = points.findIndex((point) => point.year >= activeYear);
  const activeIndex = requestedActiveIndex < 0 ? Math.max(0, points.length - 1) : requestedActiveIndex;
  const active = points[activeIndex];
  const maximum = Math.max(1, ...points.map(annualTotal));
  const plotBottom = FRAME_HEIGHT - FRAME_INSETS.bottom;
  const x = linearScale([1, Math.max(1, points.length)], [FRAME_INSETS.left, WIDTH - FRAME_INSETS.right]);
  const y = linearScale([0, maximum * 1.08], [plotBottom, FRAME_INSETS.top]);
  const plotWidth = WIDTH - FRAME_INSETS.left - FRAME_INSETS.right;
  const barWidth = Math.min(34, plotWidth / Math.max(1, points.length) * 0.62);
  const crossover = points.find((point) => point.principalPaid >= point.interestPaid);

  return (
    <section className="home-plan-frame">
      <ChartHeading
        title={crossover
          ? `From year ${crossover.year}, more of the scheduled EMI goes to principal than interest.`
          : "Interest remains the larger part of the scheduled EMI."}
        conclusion="Each annual column shows the actual rupees paid as interest, regular principal and extra principal."
      />
      <ScrubbableSvg
        width={WIDTH}
        height={FRAME_HEIGHT}
        insets={FRAME_INSETS}
        pointCount={points.length}
        activeIndex={activeIndex}
        label="Annual rupee payments split into interest, regular principal and extra principal"
        valueText={active
          ? `Year ${active.year}, ${formatCurrency(active.interestPaid, true)} interest, ${formatCurrency(active.principalPaid, true)} regular principal, ${formatCurrency(active.extraPaid, true)} extra principal`
          : undefined}
        className="home-plan-payment-mechanics"
        onPreviewIndex={(index) => onPreviewPaymentMonth(
          index == null ? null : points[index].year * 12,
        )}
        onPinIndex={(index) => onPinPaymentMonth(points[index].year * 12)}
      >
        {[0, maximum / 2, maximum].map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={FRAME_INSETS.left}
              x2={WIDTH - FRAME_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={FRAME_INSETS.left - 9} y={y.map(value) + 4}>{formatCurrency(value, true)}</text>
          </g>
        ))}
        {points.map((point, index) => {
          const columnX = x.map(point.year) - barWidth / 2;
          const principalTop = y.map(point.principalPaid);
          const interestTop = y.map(point.principalPaid + point.interestPaid);
          const totalTop = y.map(annualTotal(point));
          return (
            <g
              key={point.year}
              className={`home-plan-amount-bar ${index === activeIndex ? "is-active" : ""}`}
              aria-hidden="true"
            >
              <rect
                className="is-principal"
                x={columnX}
                y={principalTop}
                width={barWidth}
                height={plotBottom - principalTop}
              />
              <rect
                className="is-interest"
                x={columnX}
                y={interestTop}
                width={barWidth}
                height={principalTop - interestTop}
              />
              <rect
                className="is-extra"
                x={columnX}
                y={totalTop}
                width={barWidth}
                height={interestTop - totalTop}
              />
              <rect
                className="is-outline"
                x={columnX}
                y={totalTop}
                width={barWidth}
                height={plotBottom - totalTop}
              />
            </g>
          );
        })}
        {chartTickIndexes(points.length, 6).map((index) => (
          <text key={points[index].year} x={x.map(points[index].year)} y={FRAME_HEIGHT - 11} className="home-plan-chart-axis">
            {points[index].year}y
          </text>
        ))}
      </ScrubbableSvg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-interest">Interest</span>
        <span className="is-principal">Regular principal</span>
        <span className="is-extra">Extra principal</span>
      </div>
      {active ? (
        <ChartReadout columns={4}>
          <ReadoutValue label="Year" value={String(active.year)} />
          <ReadoutValue label="Interest" value={formatCurrency(active.interestPaid, true)} tone="interest" />
          <ReadoutValue label="Regular principal" value={formatCurrency(active.principalPaid, true)} tone="principal" />
          <ReadoutValue label="Extra principal" value={formatCurrency(active.extraPaid, true)} />
        </ChartReadout>
      ) : null}
    </section>
  );
}

function TimingImpact({
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
  const points = model.oneOffExtraPaymentCurve;
  const activeYear = Math.max(1, Math.ceil(activePaymentMonth / 12));
  const requestedActiveIndex = points.findIndex((point) => point.year >= activeYear);
  const activeIndex = requestedActiveIndex < 0 ? Math.max(0, points.length - 1) : requestedActiveIndex;
  const active = points[activeIndex];
  const maximum = Math.max(1, ...points.map((point) => point.interestSaved));
  const plotBottom = FRAME_HEIGHT - FRAME_INSETS.bottom;
  const maximumYear = Math.max(1, points.at(-1)?.year ?? 1);
  const x = linearScale([1, maximumYear], [FRAME_INSETS.left, WIDTH - FRAME_INSETS.right]);
  const y = linearScale([0, maximum * 1.12], [plotBottom, FRAME_INSETS.top]);
  const halfYear = model.markers.halfFirstYearImpactYear;
  const halfPoint = halfYear == null ? undefined : points.find((point) => point.year === halfYear);

  const pointLabel = (point: OneOffExtraPaymentPoint) => (
    `${point.year === 1 ? "Year 1" : "Half impact"} · ${formatCurrency(point.interestSaved, true)}`
  );

  return (
    <section className="home-plan-frame">
      <ChartHeading
        title={halfYear == null
          ? "One extra EMI is most useful early in the loan."
          : `One extra EMI loses half its interest-saving impact by year ${halfYear}.`}
        conclusion="Every point tests one additional EMI in that year against the loan with no extras."
      />
      <ScrubbableSvg
        width={WIDTH}
        height={FRAME_HEIGHT}
        insets={FRAME_INSETS}
        pointCount={points.length}
        activeIndex={activeIndex}
        label="Interest avoided by paying one additional EMI in each possible year"
        valueText={active
          ? `Year ${active.year}, one extra EMI ${formatCurrency(active.extraPaid, true)}, ${formatCurrency(active.interestSaved, true)} interest avoided`
          : undefined}
        className="home-plan-timing-impact"
        onPreviewIndex={(index) => onPreviewPaymentMonth(index == null ? null : points[index].year * 12)}
        onPinIndex={(index) => onPinPaymentMonth(points[index].year * 12)}
      >
        {[0, maximum / 2, maximum].map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={FRAME_INSETS.left}
              x2={WIDTH - FRAME_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={FRAME_INSETS.left - 9} y={y.map(value) + 4}>{formatCurrency(value, true)}</text>
          </g>
        ))}
        {points.map((point, index) => (
          <g
            key={point.year}
            className={`home-plan-lollipop ${index === activeIndex ? "is-active" : ""}`}
            aria-hidden="true"
          >
            <line
              x1={x.map(point.year)}
              x2={x.map(point.year)}
              y1={y.map(0)}
              y2={y.map(point.interestSaved)}
            />
            <circle cx={x.map(point.year)} cy={y.map(point.interestSaved)} r={index === activeIndex ? 6 : 4} />
          </g>
        ))}
        {points[0] ? (
          <text
            x={x.map(points[0].year) + 8}
            y={y.map(points[0].interestSaved) - 9}
            className="home-plan-impact-label"
          >
            {pointLabel(points[0])}
          </text>
        ) : null}
        {halfPoint ? (
          <text
            x={x.map(halfPoint.year) + 8}
            y={y.map(halfPoint.interestSaved) - 9}
            className="home-plan-impact-label"
          >
            {pointLabel(halfPoint)}
          </text>
        ) : null}
        {chartTickIndexes(points.length, 6).map((index) => (
          <text key={points[index].year} x={x.map(points[index].year)} y={FRAME_HEIGHT - 11} className="home-plan-chart-axis">
            {points[index].year}y
          </text>
        ))}
      </ScrubbableSvg>
      {active ? (
        <ChartReadout columns={4}>
          <ReadoutValue label="Timing" value={`Year ${active.year}`} />
          <ReadoutValue label="One extra EMI" value={formatCurrency(active.extraPaid, true)} />
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

function Sparkline({
  points,
  className,
}: {
  points: MonthlyRepaymentPoint[];
  className: string;
}) {
  const checkpoints = yearlyCheckpoints(points);
  const width = 220;
  const height = 58;
  const maximumMonth = Math.max(1, checkpoints.at(-1)?.paymentNumber ?? 1);
  const maximumEmi = Math.max(1, ...checkpoints.map((point) => point.scheduledEmi));
  const x = linearScale([1, maximumMonth], [2, width - 2]);
  const y = linearScale([0, maximumEmi], [height - 3, 3]);
  const path: ChartPoint[] = checkpoints.map((point) => ({
    x: x.map(point.paymentNumber),
    y: y.map(point.scheduledEmi),
  }));
  return (
    <svg viewBox={`0 0 ${width} ${height}`} aria-hidden="true" className="home-plan-strategy-sparkline">
      <path d={smoothLinePath(path)} className={`home-plan-curve ${className}`} />
    </svg>
  );
}

function firstRecalculatedEmi(points: MonthlyRepaymentPoint[]): number {
  const extraIndex = points.findIndex((point) => point.extraPaid > 0);
  if (extraIndex < 0) return points[0]?.scheduledEmi ?? 0;
  return points[extraIndex + 1]?.scheduledEmi ?? points[extraIndex].scheduledEmi;
}

function StrategyCards({
  stories,
  model,
  activePaymentMonth,
  onSelect,
}: {
  stories: RepaymentChartStories;
  model: RepaymentDashboardModel;
  activePaymentMonth: number;
  onSelect: (strategy: RepaymentDashboardModel["strategy"]) => void;
}) {
  const finishSummary = model.strategyComparison.find((point) => point.strategy === "finish_earlier");
  const lowerSummary = model.strategyComparison.find((point) => point.strategy === "lower_emi");
  const finishPayoff = stories.finishEarlierMonthly.at(-1)?.paymentNumber ?? 0;
  const lowerPayoff = stories.lowerEmiMonthly.at(-1)?.paymentNumber ?? 0;
  const lowerFirstEmi = firstRecalculatedEmi(stories.lowerEmiMonthly);
  const lowerSelectedEmi = activeMonthlyPoint(stories.lowerEmiMonthly, activePaymentMonth)?.scheduledEmi
    ?? lowerFirstEmi;
  const hasExtras = model.extraEmisPerYear > 0;

  return (
    <section className="home-plan-frame home-plan-frame--strategies">
      <ChartHeading
        title="Same prepayment rule, two repayment objectives."
        conclusion={hasExtras
          ? "Choose the outcome that fits your cash-flow priority."
          : "Add an annual extra EMI above to see the objectives diverge."}
      />
      <div className="home-plan-strategy-cards">
        <button
          type="button"
          className={model.strategy === "finish_earlier" ? "is-active" : undefined}
          aria-pressed={model.strategy === "finish_earlier"}
          onClick={() => onSelect("finish_earlier")}
        >
          <header>
            <strong>Finish earlier</strong>
            <span>Keep the scheduled EMI</span>
          </header>
          <Sparkline points={stories.finishEarlierMonthly} className="is-finish-earlier" />
          <dl>
            <div><dt>Scheduled EMI</dt><dd>{formatCurrency(model.openingMonthlyEmi)}</dd></div>
            <div><dt>Payoff</dt><dd>{durationLabel(finishPayoff)}</dd></div>
            <div><dt>Interest avoided</dt><dd>{formatCurrency(finishSummary?.interestSaved ?? 0, true)}</dd></div>
            <div><dt>Primary benefit</dt><dd>{durationLabel(finishSummary?.monthsSaved ?? 0)} sooner</dd></div>
          </dl>
        </button>
        <button
          type="button"
          className={model.strategy === "lower_emi" ? "is-active" : undefined}
          aria-pressed={model.strategy === "lower_emi"}
          onClick={() => onSelect("lower_emi")}
        >
          <header>
            <strong>Lower EMI</strong>
            <span>Keep the original payoff date</span>
          </header>
          <Sparkline points={stories.lowerEmiMonthly} className="is-lower-emi" />
          <dl>
            <div><dt>After first prepayment</dt><dd>{formatCurrency(lowerFirstEmi)}</dd></div>
            <div><dt>At {loanTimeLabel(activePaymentMonth).toLowerCase()}</dt><dd>{formatCurrency(lowerSelectedEmi)}</dd></div>
            <div><dt>Payoff</dt><dd>{durationLabel(lowerPayoff)}</dd></div>
            <div><dt>Interest avoided</dt><dd>{formatCurrency(lowerSummary?.interestSaved ?? 0, true)}</dd></div>
          </dl>
        </button>
      </div>
    </section>
  );
}

function CalculationDisclosure({
  model,
}: {
  model: RepaymentDashboardModel;
}) {
  return (
    <details className="home-plan-calculation">
      <summary>How the calculation works</summary>
      <div>
        <p>
          Interest is calculated monthly on outstanding principal. Annual extra payments go
          directly to principal; one extra EMI means the selected count multiplied by the
          scheduled EMI in that year.
        </p>
        <code>EMI = P × r × (1 + r)ⁿ ÷ ((1 + r)ⁿ − 1)</code>
        <p>
          Here P is principal, r is the monthly interest rate and n is the remaining number
          of monthly payments. At a zero interest rate, EMI is P ÷ n.
        </p>
        <p>
          Finish earlier keeps the scheduled EMI constant and recalculates tenure. Lower EMI
          keeps the original payoff date and recalculates the EMI, so future extra payments
          also fall. The {formatCurrency(model.interestSaved, true)} loan-interest saving shown
          above comes from the repayment schedule; Rent vs Buy market returns are projections,
          not guaranteed savings.
        </p>
      </div>
    </details>
  );
}

export function RepaymentDashboard({
  inputs,
  model,
  onStrategyChange,
}: RepaymentDashboardProps) {
  const stories = useMemo(() => buildRepaymentChartStories(inputs, model), [inputs, model]);
  const [previewPaymentMonth, setPreviewPaymentMonth] = useState<number | null>(null);
  const [pinnedPaymentMonth, setPinnedPaymentMonth] = useState(12);
  const activePaymentMonth = previewPaymentMonth ?? pinnedPaymentMonth;

  if (stories.baselineMonthly.length === 0) {
    return (
      <section className="home-plan-frame">
        <ChartHeading
          title="No loan is needed with this down payment."
          conclusion="Reduce the down payment above to model a repayment plan."
        />
      </section>
    );
  }

  return (
    <div className="home-plan-journey">
      <BalanceOverview
        key={`${inputs.downPaymentPercent}-${inputs.loanRate}-${inputs.monthlyEmiThousands}-${model.extraEmisPerYear}-${model.strategy}`}
        stories={stories}
        model={model}
        activePaymentMonth={activePaymentMonth}
        onPreviewPaymentMonth={setPreviewPaymentMonth}
        onPinPaymentMonth={setPinnedPaymentMonth}
      />
      <PaymentMechanics
        model={model}
        activePaymentMonth={activePaymentMonth}
        onPreviewPaymentMonth={setPreviewPaymentMonth}
        onPinPaymentMonth={setPinnedPaymentMonth}
      />
      <TimingImpact
        model={model}
        activePaymentMonth={activePaymentMonth}
        onPreviewPaymentMonth={setPreviewPaymentMonth}
        onPinPaymentMonth={setPinnedPaymentMonth}
      />
      <StrategyCards
        stories={stories}
        model={model}
        activePaymentMonth={activePaymentMonth}
        onSelect={onStrategyChange}
      />
      <CalculationDisclosure model={model} />
    </div>
  );
}
