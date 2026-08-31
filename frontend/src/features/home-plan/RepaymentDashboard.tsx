import { useMemo, useState } from "react";
import {
  formatCurrency,
  formatLakhCurrency,
  formatMonthlyCurrency,
  type PlanInputs,
} from "./model.ts";
import type {
  OneOffExtraPaymentPoint,
  RepaymentDashboardModel,
  RepaymentYearPoint,
} from "./repaymentModel.ts";
import { RepaymentScheduleSheet } from "./RepaymentScheduleSheet.tsx";
import { aggregateRepaymentSchedule } from "./repaymentSchedule.ts";
import {
  buildRepaymentChartStories,
  type MonthlyRepaymentPoint,
  type RepaymentChartStories,
} from "./chartStories.ts";
import {
  ChartHeading,
  ScrubbableSvg,
} from "./charts/ChartPrimitives.tsx";
import {
  areaPath,
  bandPath,
  chartTickIndexes,
  linearScale,
  smoothLinePath,
  type ChartPoint,
} from "./charts/chartGeometry.ts";

const PRIMARY_WIDTH = 1_000;
const PRIMARY_HEIGHT = 260;
const SUPPORT_WIDTH = 560;
const SUPPORT_HEIGHT = 250;
const PRIMARY_INSETS = { top: 28, right: 78, bottom: 38, left: 70 };
const SUPPORT_INSETS = { top: 30, right: 28, bottom: 38, left: 64 };

type RepaymentDashboardProps = {
  inputs: PlanInputs;
  model: RepaymentDashboardModel;
};

function durationLabel(months: number | null | undefined): string {
  if (months == null) return "Not repaid";
  const years = Math.floor(months / 12);
  const remainder = months % 12;
  if (years === 0) return `${remainder} mo`;
  if (remainder === 0) return `${years} yr`;
  return `${years} yr ${remainder} mo`;
}

function activeMonthlyPoint(
  points: MonthlyRepaymentPoint[],
  paymentMonth: number,
): MonthlyRepaymentPoint | undefined {
  if (points.length === 0 || paymentMonth > (points.at(-1)?.paymentNumber ?? 0)) return undefined;
  return points[Math.max(0, Math.min(paymentMonth - 1, points.length - 1))];
}

function balanceAt(points: MonthlyRepaymentPoint[], paymentMonth: number): number {
  return activeMonthlyPoint(points, paymentMonth)?.closingBalance ?? 0;
}

function tickYears(horizonYears: number, count = 5): number[] {
  return chartTickIndexes(horizonYears + 1, count);
}

function loanYearFromPointer(
  clientX: number,
  bounds: DOMRect,
  width: number,
  x: ReturnType<typeof linearScale>,
): number {
  const svgX = (clientX - bounds.left) / Math.max(1, bounds.width) * width;
  return Math.round(x.invert(svgX));
}

function OutcomeStrip({
  model,
  onViewSchedule,
}: {
  model: RepaymentDashboardModel;
  onViewSchedule: () => void;
}) {
  const noExtras = model.extraEmisPerYear === 0;
  const becomesRepayable = model.baselinePayoffMonths == null
    && model.selectedPayoffMonths != null;
  const action = `${model.extraEmisPerYear} extra payment${model.extraEmisPerYear === 1 ? "" : "s"}/year`;
  const before = model.strategy === "finish_earlier"
    ? durationLabel(model.baselinePayoffMonths)
    : `${formatMonthlyCurrency(model.openingMonthlyEmi)} now`;
  const after = model.strategy === "finish_earlier"
    ? durationLabel(model.selectedPayoffMonths)
    : `${formatMonthlyCurrency(model.firstYearRecalculatedMonthlyEmi)} after year 1`;
  const benefit = noExtras
    ? "Choose extra payments above to compare a repayment plan."
    : !model.comparisonAvailable
      ? becomesRepayable
        ? "The selected plan becomes repayable; lifetime interest is not comparable."
        : "The loan does not close within the modelled horizon."
      : model.strategy === "finish_earlier"
        ? `Save ${durationLabel(model.monthsSaved)} and ${formatCurrency(model.interestSaved, true)} interest`
        : `EMI keeps stepping down as the fixed extras recur each year; the original payoff date stays unchanged and estimated interest falls by ${formatCurrency(model.interestSaved, true)}`;

  return (
    <section className="home-plan-outcome-strip" aria-label="Selected repayment outcome">
      <span>Selected plan</span>
      <strong>{action}</strong>
      <div className="home-plan-outcome-strip__change" aria-label={`${before} changes to ${after}`}>
        <b>{before}</b>
        <span aria-hidden="true">→</span>
        <b>{after}</b>
      </div>
      <p>{benefit}</p>
      <div className="home-plan-outcome-strip__schedule">
        <button type="button" onClick={onViewSchedule}>
          <span aria-hidden="true">▤</span>
          View yearly schedule
          <span aria-hidden="true">›</span>
        </button>
        <small>Inspect every year and expand any year into monthly payments.</small>
      </div>
    </section>
  );
}

function BalanceChart({
  stories,
  model,
  horizonMonths,
  baselinePayoffMonths,
  selectedPayoffMonths,
  activeYear,
  onPreviewYear,
  onPinYear,
}: {
  stories: RepaymentChartStories;
  model: RepaymentDashboardModel;
  horizonMonths: number;
  baselinePayoffMonths: number | null;
  selectedPayoffMonths: number | null;
  activeYear: number;
  onPreviewYear: (year: number | null) => void;
  onPinYear: (year: number) => void;
}) {
  const baseline = stories.baselineMonthly;
  const selected = stories.selectedMonthly;
  const horizonYears = Math.max(1, Math.ceil(horizonMonths / 12));
  const checkpointMonths = [...new Set([
    1,
    ...Array.from({ length: horizonYears }, (_, index) => Math.min(horizonMonths, (index + 1) * 12)),
    baselinePayoffMonths ?? horizonMonths,
    selectedPayoffMonths ?? horizonMonths,
  ])].sort((left, right) => left - right);
  const maximumBalance = Math.max(
    1,
    ...baseline.map((point) => point.closingBalance),
    ...selected.map((point) => point.closingBalance),
  );
  const plotBottom = PRIMARY_HEIGHT - PRIMARY_INSETS.bottom;
  const x = linearScale([0, horizonYears], [PRIMARY_INSETS.left, PRIMARY_WIDTH - PRIMARY_INSETS.right]);
  const y = linearScale([0, maximumBalance * 1.04], [plotBottom, PRIMARY_INSETS.top]);
  const baselinePath = checkpointMonths.map((month) => ({
    x: x.map(month / 12),
    y: y.map(balanceAt(baseline, month)),
  }));
  const selectedPath = checkpointMonths.map((month) => ({
    x: x.map(month / 12),
    y: y.map(balanceAt(selected, month)),
  }));
  const selectedLinePath = selectedPath.filter((_, index) => (
    selectedPayoffMonths == null || checkpointMonths[index] <= selectedPayoffMonths
  ));
  const activeMonth = Math.min(horizonMonths, Math.max(1, activeYear * 12));
  const activeBalance = balanceAt(selected, activeMonth);
  const hasComparison = model.extraEmisPerYear > 0;
  const samePayoff = baselinePayoffMonths != null
    && baselinePayoffMonths === selectedPayoffMonths;

  return (
    <section className="home-plan-primary-chart">
      <ChartHeading
        title="Loan balance over time"
        conclusion="Original and selected plan across the original loan horizon."
      />
      <ScrubbableSvg
        width={PRIMARY_WIDTH}
        height={PRIMARY_HEIGHT}
        insets={PRIMARY_INSETS}
        pointCount={horizonYears + 1}
        activeIndex={activeYear}
        label="Selected and original outstanding loan balance by year"
        valueText={`Year ${activeYear}, selected balance ${formatCurrency(activeBalance, true)}`}
        className="home-plan-balance-overview"
        indexFromPoint={(clientX, _clientY, bounds) => (
          loanYearFromPointer(clientX, bounds, PRIMARY_WIDTH, x)
        )}
        onPreviewIndex={onPreviewYear}
        onPinIndex={onPinYear}
      >
        {[0, maximumBalance / 2, maximumBalance].map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={PRIMARY_INSETS.left}
              x2={PRIMARY_WIDTH - PRIMARY_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={PRIMARY_INSETS.left - 9} y={y.map(value) + 4}>
              {formatCurrency(value, true)}
            </text>
          </g>
        ))}
        {hasComparison ? (
          <path d={bandPath(selectedPath, baselinePath)} className="home-plan-balance-gap" />
        ) : null}
        <path d={smoothLinePath(baselinePath)} className="home-plan-curve is-baseline" />
        {hasComparison ? (
          <path d={smoothLinePath(selectedLinePath)} className="home-plan-curve is-selected" />
        ) : null}
        {tickYears(horizonYears).map((year) => (
          <text key={year} x={x.map(year)} y={PRIMARY_HEIGHT - 10} className="home-plan-chart-axis">
            {year === 0 ? "Now" : `${year}y`}
          </text>
        ))}
        {baselinePayoffMonths != null ? (
          <g className="home-plan-payoff-marker is-baseline-payoff" aria-hidden="true">
            <line
              x1={x.map(baselinePayoffMonths / 12)}
              x2={x.map(baselinePayoffMonths / 12)}
              y1={PRIMARY_INSETS.top}
              y2={plotBottom}
            />
            <text
              x={x.map(baselinePayoffMonths / 12) - 10}
              y={plotBottom - 14}
              textAnchor="end"
            >
              {samePayoff ? "Both plans" : "Original payoff"}
            </text>
          </g>
        ) : null}
        {hasComparison && selectedPayoffMonths != null && !samePayoff ? (
          <g className="home-plan-payoff-marker is-selected-payoff" aria-hidden="true">
            <line
              x1={x.map(selectedPayoffMonths / 12)}
              x2={x.map(selectedPayoffMonths / 12)}
              y1={PRIMARY_INSETS.top}
              y2={plotBottom}
            />
            <circle cx={x.map(selectedPayoffMonths / 12)} cy={y.map(0)} r="4" />
            <text
              x={x.map(selectedPayoffMonths / 12) + 10}
              y={plotBottom - 14}
              textAnchor="start"
            >
              Selected payoff
            </text>
          </g>
        ) : null}
        <g className="home-plan-chart-cursor">
          <line
            x1={x.map(activeYear)}
            x2={x.map(activeYear)}
            y1={PRIMARY_INSETS.top}
            y2={plotBottom}
          />
          <circle cx={x.map(activeYear)} cy={y.map(activeBalance)} r="4.5" />
        </g>
      </ScrubbableSvg>
    </section>
  );
}

function emptyRepaymentYear(year: number): RepaymentYearPoint {
  return {
    year,
    scheduledMonthlyEmi: 0,
    scheduledPaid: 0,
    interestPaid: 0,
    principalPaid: 0,
    extraPaid: 0,
    balance: 0,
  };
}

function annualTotal(point: RepaymentYearPoint): number {
  return point.interestPaid + point.principalPaid + point.extraPaid;
}

function PaymentCompositionChart({
  model,
  horizonYears,
  activeYear,
  onPreviewYear,
  onPinYear,
}: {
  model: RepaymentDashboardModel;
  horizonYears: number;
  activeYear: number;
  onPreviewYear: (year: number | null) => void;
  onPinYear: (year: number) => void;
}) {
  const points = Array.from({ length: horizonYears }, (_, index) => (
    model.recurrentSchedule.find((point) => point.year === index + 1)
      ?? emptyRepaymentYear(index + 1)
  ));
  const active = points[Math.max(0, Math.min(activeYear - 1, points.length - 1))];
  const maximum = Math.max(1, ...points.map(annualTotal));
  const plotBottom = SUPPORT_HEIGHT - SUPPORT_INSETS.bottom;
  const x = linearScale([0.5, horizonYears + 0.5], [SUPPORT_INSETS.left, SUPPORT_WIDTH - SUPPORT_INSETS.right]);
  const y = linearScale([0, maximum * 1.08], [plotBottom, SUPPORT_INSETS.top]);
  const plotWidth = SUPPORT_WIDTH - SUPPORT_INSETS.left - SUPPORT_INSETS.right;
  const barWidth = Math.max(6, Math.min(20, plotWidth / Math.max(1, horizonYears) * 0.6));
  const crossover = model.markers.crossoverYear;

  return (
    <section className="home-plan-support-chart">
      <ChartHeading
        title="Where your payments go"
        conclusion={crossover == null
          ? "Interest remains larger than scheduled principal."
          : `Scheduled principal exceeds interest in year ${crossover}.`}
      />
      <ScrubbableSvg
        width={SUPPORT_WIDTH}
        height={SUPPORT_HEIGHT}
        insets={SUPPORT_INSETS}
        pointCount={horizonYears + 1}
        activeIndex={activeYear}
        label="Annual payment composition across the original loan horizon"
        valueText={`Year ${activeYear}, ${formatCurrency(active.interestPaid, true)} interest, ${formatCurrency(active.principalPaid, true)} scheduled principal, ${formatCurrency(active.extraPaid, true)} extra principal`}
        className="home-plan-payment-mechanics"
        indexFromPoint={(clientX, _clientY, bounds) => (
          loanYearFromPointer(clientX, bounds, SUPPORT_WIDTH, x)
        )}
        onPreviewIndex={onPreviewYear}
        onPinIndex={onPinYear}
      >
        {[0, maximum / 2, maximum].map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={SUPPORT_INSETS.left}
              x2={SUPPORT_WIDTH - SUPPORT_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={SUPPORT_INSETS.left - 8} y={y.map(value) + 4}>
              {formatCurrency(value, true)}
            </text>
          </g>
        ))}
        {points.map((point) => {
          const principalTop = y.map(point.principalPaid);
          const interestTop = y.map(point.principalPaid + point.interestPaid);
          const totalTop = y.map(annualTotal(point));
          return (
            <g
              key={point.year}
              className={`home-plan-amount-bar ${point.year === activeYear ? "is-active" : ""}`}
              aria-hidden="true"
            >
              <rect
                className="is-principal"
                x={x.map(point.year) - barWidth / 2}
                y={principalTop}
                width={barWidth}
                height={plotBottom - principalTop}
              />
              <rect
                className="is-interest"
                x={x.map(point.year) - barWidth / 2}
                y={interestTop}
                width={barWidth}
                height={principalTop - interestTop}
              />
              <rect
                className="is-extra"
                x={x.map(point.year) - barWidth / 2}
                y={totalTop}
                width={barWidth}
                height={interestTop - totalTop}
              />
            </g>
          );
        })}
        {crossover != null ? (
          <g className="home-plan-milestone-marker" aria-hidden="true">
            <line
              x1={x.map(crossover)}
              x2={x.map(crossover)}
              y1={SUPPORT_INSETS.top}
              y2={plotBottom}
            />
            <text x={x.map(crossover)} y={SUPPORT_INSETS.top - 7}>Principal crossover</text>
          </g>
        ) : null}
        {tickYears(horizonYears, 4).map((year) => (
          <text key={year} x={x.map(Math.max(1, year))} y={SUPPORT_HEIGHT - 10} className="home-plan-chart-axis">
            {year === 0 ? "1y" : `${year}y`}
          </text>
        ))}
      </ScrubbableSvg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-interest">Interest</span>
        <span className="is-principal">Scheduled principal</span>
        <span className="is-extra">Extra principal</span>
      </div>
    </section>
  );
}

function zeroImpactPoint(year: number): OneOffExtraPaymentPoint {
  return {
    year,
    interestSaved: 0,
    monthsSaved: 0,
    monthlyEmiReduction: 0,
    extraPaid: 0,
  };
}

function TimingImpactChart({
  model,
  horizonYears,
  activeYear,
  onPreviewYear,
  onPinYear,
}: {
  model: RepaymentDashboardModel;
  horizonYears: number;
  activeYear: number;
  onPreviewYear: (year: number | null) => void;
  onPinYear: (year: number) => void;
}) {
  const points = model.oneOffExtraPaymentCurve;
  const active = points.find((point) => point.year === activeYear) ?? zeroImpactPoint(activeYear);
  const first = points[0];
  const final = points.at(-1);
  const half = model.markers.halfFirstYearImpactYear == null
    ? undefined
    : points.find((point) => point.year === model.markers.halfFirstYearImpactYear);
  const maximum = Math.max(1, ...points.map((point) => point.interestSaved));
  const plotBottom = SUPPORT_HEIGHT - SUPPORT_INSETS.bottom;
  const x = linearScale([0, horizonYears], [SUPPORT_INSETS.left, SUPPORT_WIDTH - SUPPORT_INSETS.right]);
  const y = linearScale([0, maximum * 1.14], [plotBottom, SUPPORT_INSETS.top]);
  const path: ChartPoint[] = points.map((point) => ({
    x: x.map(point.year),
    y: y.map(point.interestSaved),
  }));
  const highlighted = [...new Map(
    [first, half, active.extraPaid > 0 ? active : undefined, final]
      .filter((point): point is OneOffExtraPaymentPoint => point != null)
      .map((point) => [point.year, point]),
  ).values()];

  return (
    <section className="home-plan-support-chart">
      <ChartHeading
        title="Why early extra payments save more"
        conclusion={!model.comparisonAvailable
          ? "Lifetime interest impact is not comparable because the original EMI never repays the loan."
          : half == null
          ? "Earlier extra payments avoid more lifetime interest."
          : `Year ${half.year}: an extra payment saves half as much interest as in Year 1.`}
      />
      <ScrubbableSvg
        width={SUPPORT_WIDTH}
        height={SUPPORT_HEIGHT}
        insets={SUPPORT_INSETS}
        pointCount={horizonYears + 1}
        activeIndex={activeYear}
        label="Lifetime interest avoided by one extra payment across the original loan horizon"
        valueText={!model.comparisonAvailable
          ? `Year ${activeYear}, lifetime interest comparison unavailable`
          : active.extraPaid > 0
          ? `Year ${activeYear}, one extra payment ${formatCurrency(active.extraPaid, true)}, ${formatCurrency(active.interestSaved, true)} lifetime interest avoided`
          : `Year ${activeYear}, no scheduled extra payment remains`}
        className="home-plan-timing-impact"
        indexFromPoint={(clientX, _clientY, bounds) => (
          loanYearFromPointer(clientX, bounds, SUPPORT_WIDTH, x)
        )}
        onPreviewIndex={onPreviewYear}
        onPinIndex={onPinYear}
      >
        {half ? (
          <rect
            x={x.map(0)}
            y={SUPPORT_INSETS.top}
            width={Math.max(0, x.map(half.year) - x.map(0))}
            height={plotBottom - SUPPORT_INSETS.top}
            className="home-plan-high-impact-window"
          />
        ) : null}
        {(model.comparisonAvailable ? [0, maximum / 2, maximum] : []).map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line
              x1={SUPPORT_INSETS.left}
              x2={SUPPORT_WIDTH - SUPPORT_INSETS.right}
              y1={y.map(value)}
              y2={y.map(value)}
            />
            <text x={SUPPORT_INSETS.left - 8} y={y.map(value) + 4}>
              {formatCurrency(value, true)}
            </text>
          </g>
        ))}
        {points.length > 0 ? (
          <>
            <path d={areaPath(path, plotBottom)} className="home-plan-impact-area" />
            <path d={smoothLinePath(path)} className="home-plan-curve is-timing" />
          </>
        ) : (
          <text
            x={(SUPPORT_INSETS.left + SUPPORT_WIDTH - SUPPORT_INSETS.right) / 2}
            y={(SUPPORT_INSETS.top + plotBottom) / 2}
            className="home-plan-chart-empty"
          >
            Lifetime comparison unavailable
          </text>
        )}
        {highlighted.map((point) => (
          <g key={point.year} className="home-plan-impact-point" aria-hidden="true">
            <circle
              cx={x.map(point.year)}
              cy={y.map(point.interestSaved)}
              r={point.year === activeYear ? 5.5 : 4}
            />
            {point === first || point === half || point === final ? (
              <text
                x={x.map(point.year)}
                y={Math.max(SUPPORT_INSETS.top + 11, y.map(point.interestSaved) - 14)}
                textAnchor={point === final ? "end" : point === first ? "start" : "middle"}
              >
                {point === first ? "Year 1" : point === half ? "Half impact" : "Final useful year"}
              </text>
            ) : null}
          </g>
        ))}
        {model.comparisonAvailable && active.extraPaid === 0 ? (
          <circle
            cx={x.map(activeYear)}
            cy={y.map(0)}
            r="5"
            className="home-plan-empty-impact-point"
          />
        ) : null}
        {tickYears(horizonYears, 4).map((year) => (
          <text key={year} x={x.map(year)} y={SUPPORT_HEIGHT - 10} className="home-plan-chart-axis">
            {year === 0 ? "Now" : `${year}y`}
          </text>
        ))}
      </ScrubbableSvg>
    </section>
  );
}

function MilestoneRail({
  model,
}: {
  model: RepaymentDashboardModel;
}) {
  const milestones = [
    { sort: 1, value: "Year 1", label: "Interest-heavy period" },
    {
      sort: model.markers.crossoverYear ?? Number.POSITIVE_INFINITY,
      value: model.markers.crossoverYear == null ? "—" : `Year ${model.markers.crossoverYear}`,
      label: "Principal crossover",
    },
    {
      sort: model.markers.halfFirstYearImpactYear ?? Number.POSITIVE_INFINITY,
      value: model.markers.halfFirstYearImpactYear == null
        ? "—"
        : `Year ${model.markers.halfFirstYearImpactYear}`,
      label: "Extra-payment impact has halved",
    },
  ].sort((left, right) => left.sort - right.sort);

  return (
    <ol
      className="home-plan-milestone-rail"
      aria-label="Loan milestones"
    >
      {milestones.map(({ value, label }) => (
        <li key={label}>
          <strong>{value}</strong>
          <span>{label}</span>
        </li>
      ))}
    </ol>
  );
}

function AssumptionsDisclosure({
  inputs,
  model,
}: {
  inputs: PlanInputs;
  model: RepaymentDashboardModel;
}) {
  const loanAmount = inputs.propertyPriceLakh * 100_000 * (1 - inputs.downPaymentPercent / 100);
  return (
    <details className="home-plan-assumptions">
      <summary>How we estimated this</summary>
      <div className="home-plan-assumptions__body">
        <ul>
          <li>
            Modelled loan: {formatCurrency(loanAmount, true)} at {inputs.loanRate}% yearly interest.
          </li>
          <li>
            Scheduled EMI: {formatMonthlyCurrency(model.openingMonthlyEmi)}, with interest
            calculated monthly on the remaining balance.
          </li>
          <li>
            Each recurring extra payment stays equal to today&apos;s EMI—
            {" "}{formatLakhCurrency(model.openingMonthlyEmi)}—and goes directly to principal.
            Lower-EMI extras stop when another full payment would close the loan early.
          </li>
          <li>
            {model.strategy === "finish_earlier"
              ? "Shorten tenure keeps the monthly EMI unchanged and brings the payoff date forward."
              : "Lower EMI keeps the original payoff date and recalculates the monthly EMI after each extra payment."}
          </li>
          <li>
            This is an illustrative recast. A lender may default to shortening tenure, require
            a request to lower EMI, or apply product-specific prepayment limits.
          </li>
          <li>
            The half-impact marker is the first year an extra payment avoids 50% or less of
            its Year-1 interest saving. It is a comparison point, not a recommendation to stop.
          </li>
          <li>Registration, taxes, loan fees and prepayment charges are excluded.</li>
        </ul>
      </div>
    </details>
  );
}

export function RepaymentDashboard({
  inputs,
  model,
}: RepaymentDashboardProps) {
  const stories = useMemo(() => buildRepaymentChartStories(inputs, model), [inputs, model]);
  const horizonMonths = Math.max(12, model.baselineHorizonMonths);
  const horizonYears = Math.max(1, Math.ceil(horizonMonths / 12));
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [pinnedYear, setPinnedYear] = useState(1);
  const [scheduleOpen, setScheduleOpen] = useState(false);
  const activeYear = Math.max(1, Math.min(horizonYears, previewYear ?? pinnedYear));
  const schedule = useMemo(
    () => aggregateRepaymentSchedule(stories.selectedMonthly, stories.baselineMonthly),
    [stories.baselineMonthly, stories.selectedMonthly],
  );

  if (stories.baselineMonthly.length === 0) {
    return (
      <section className="home-plan-primary-chart">
        <ChartHeading
          title="No loan is needed with this down payment."
          conclusion="Reduce the down payment above to model a repayment plan."
        />
      </section>
    );
  }

  return (
    <div className="home-plan-dashboard">
      <OutcomeStrip model={model} onViewSchedule={() => setScheduleOpen(true)} />
      <BalanceChart
        key={`${inputs.downPaymentPercent}-${inputs.loanRate}-${inputs.monthlyEmiThousands}-${model.extraEmisPerYear}-${model.strategy}`}
        stories={stories}
        model={model}
        horizonMonths={horizonMonths}
        baselinePayoffMonths={model.baselinePayoffMonths}
        selectedPayoffMonths={model.selectedPayoffMonths}
        activeYear={activeYear}
        onPreviewYear={setPreviewYear}
        onPinYear={setPinnedYear}
      />
      <MilestoneRail
        model={model}
      />
      <div className="home-plan-support-grid">
        <PaymentCompositionChart
          model={model}
          horizonYears={horizonYears}
          activeYear={activeYear}
          onPreviewYear={setPreviewYear}
          onPinYear={setPinnedYear}
        />
        <TimingImpactChart
          model={model}
          horizonYears={horizonYears}
          activeYear={activeYear}
          onPreviewYear={setPreviewYear}
          onPinYear={setPinnedYear}
        />
      </div>
      <AssumptionsDisclosure inputs={inputs} model={model} />
      <RepaymentScheduleSheet
        open={scheduleOpen}
        inputs={inputs}
        model={model}
        years={schedule}
        activeYear={activeYear}
        onSelectYear={(year) => {
          setPreviewYear(null);
          setPinnedYear(year);
        }}
        onClose={() => setScheduleOpen(false)}
      />
    </div>
  );
}
