import {
  buildLoanSchedule,
  constructionPlanFor,
  type LoanSchedule,
  type LoanRepaymentStatus,
  type RepaymentStrategy,
} from "./financeEngine.ts";
import type { PlanInputs } from "./model.ts";
import {
  DEFAULT_PLAN_MODEL_CONFIG,
  type PlanModelConfig,
} from "./modelConfig.ts";

const MONTHS_IN_YEAR = 12;

export type RepaymentYearPoint = {
  year: number;
  scheduledMonthlyEmi: number;
  scheduledPaid: number;
  interestPaid: number;
  principalPaid: number;
  extraPaid: number;
  balance: number;
};

/** Impact of making the selected extra payment in this repayment year only. */
export type OneOffExtraPaymentPoint = {
  year: number;
  interestSaved: number;
  monthsSaved: number;
  monthlyEmiReduction: number;
  extraPaid: number;
};

/** Impact of starting the selected annual extra-EMI cadence in this year. */
export type CadenceStartPoint = {
  startYear: number;
  interestSaved: number;
  monthsSaved: number;
  monthlyEmiReduction: number;
  extraPaid: number;
};

/** @deprecated Compatibility shape for the current chart. */
export type PrepaymentRunPoint = OneOffExtraPaymentPoint & {
  throughYear: number;
  /** @deprecated This is now the independent one-off saving, not a cumulative delta. */
  incrementalInterestSaved: number;
};

export type CadenceComparisonPoint = {
  extraEmisPerYear: 0 | 1 | 2 | 3 | 4 | 6;
  interestSaved: number;
  monthsSaved: number;
  endingMonthlyEmi: number;
  totalExtraPaid: number;
};

export type StrategyComparisonPoint = {
  strategy: RepaymentStrategy;
  interestSaved: number;
  monthsSaved: number;
  endingMonthlyEmi: number;
  totalExtraPaid: number;
};

export type RepaymentMarkers = {
  crossoverYear: number | null;
  halfFirstYearImpactYear: number | null;
  halfCadenceImpactStartYear: number | null;
};

export type RepaymentDashboardModel = {
  status: LoanRepaymentStatus;
  strategy: RepaymentStrategy;
  extraEmisPerYear: number;
  openingMonthlyEmi: number;
  endingMonthlyEmi: number;
  firstRecalculatedMonthlyEmi: number;
  annualPrepayment: number;
  interestSaved: number;
  monthsSaved: number;
  recurrentSchedule: RepaymentYearPoint[];
  oneOffExtraPaymentCurve: OneOffExtraPaymentPoint[];
  cadenceStartCurve: CadenceStartPoint[];
  cadenceComparison: CadenceComparisonPoint[];
  strategyComparison: StrategyComparisonPoint[];
  markers: RepaymentMarkers;
  /** @deprecated Use `recurrentSchedule`. */
  repaymentYears: RepaymentYearPoint[];
  /** @deprecated Use `oneOffExtraPaymentCurve`. */
  prepaymentRun: PrepaymentRunPoint[];
  crossoverYear: number | null;
  halfImpactYear: number | null;
};

function repaymentYearsFor(
  schedule: LoanSchedule,
  possessionMonth: number,
): RepaymentYearPoint[] {
  const byYear = new Map<number, RepaymentYearPoint>();
  for (const month of schedule.months) {
    if (month.paymentNumber <= 0) continue;
    const year = Math.ceil(month.paymentNumber / MONTHS_IN_YEAR);
    const point = byYear.get(year) ?? {
      year,
      scheduledMonthlyEmi: month.scheduledEmi,
      scheduledPaid: 0,
      interestPaid: 0,
      principalPaid: 0,
      extraPaid: 0,
      balance: month.openingBalance,
    };
    point.scheduledPaid += month.scheduledPayment;
    point.interestPaid += month.interestPaid;
    point.principalPaid += month.principalPaid;
    point.extraPaid += month.extraPaid;
    point.balance = month.closingBalance;
    byYear.set(year, point);
  }

  if (byYear.size === 0 && schedule.baselinePayoffMonth != null) {
    const remainingYears = Math.max(
      0,
      Math.ceil((schedule.baselinePayoffMonth - possessionMonth) / MONTHS_IN_YEAR),
    );
    return Array.from({ length: remainingYears }, (_, index) => ({
      year: index + 1,
      scheduledMonthlyEmi: 0,
      scheduledPaid: 0,
      interestPaid: 0,
      principalPaid: 0,
      extraPaid: 0,
      balance: 0,
    }));
  }

  return [...byYear.values()];
}

function interestSavedAgainst(baseline: LoanSchedule, candidate: LoanSchedule): number {
  return baseline.totalInterest != null && candidate.totalInterest != null
    ? Math.max(0, baseline.totalInterest - candidate.totalInterest)
    : 0;
}

function monthsSavedAgainst(baseline: LoanSchedule, candidate: LoanSchedule): number {
  return baseline.payoffMonth != null && candidate.payoffMonth != null
    ? Math.max(0, baseline.payoffMonth - candidate.payoffMonth)
    : 0;
}

function totalExtraPaid(schedule: LoanSchedule): number {
  return schedule.months.reduce((sum, month) => sum + month.extraPaid, 0);
}

export function calculateRepaymentDashboard(
  inputs: PlanInputs,
  extraEmisPerYear: number,
  strategy: RepaymentStrategy,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): RepaymentDashboardModel {
  const baseline = buildLoanSchedule(inputs, { extraEmisPerYear: 0 }, config);
  const selected = buildLoanSchedule(inputs, { extraEmisPerYear, strategy }, config);
  const construction = constructionPlanFor(inputs, config);
  const baselineYears = baseline.payoffMonth == null
    ? config.simulation.maximumJourneyYears
    : Math.max(
      0,
      Math.ceil((baseline.payoffMonth - construction.possessionMonth) / MONTHS_IN_YEAR),
    );
  const maximumRunYears = Math.min(baselineYears, config.simulation.maximumJourneyYears);
  const oneOffExtraPaymentCurve: OneOffExtraPaymentPoint[] = [];
  // Timing is deliberately independent from the recurring plan: it asks what
  // one additional EMI achieves when paid in this year and nowhere else.
  for (let year = 1; year <= maximumRunYears; year += 1) {
    const candidate = buildLoanSchedule(inputs, {
      extraEmisPerYear: 1,
      strategy,
      oneOffExtraPaymentYear: year,
    }, config);
    const extraPaid = totalExtraPaid(candidate);
    if (extraPaid <= 0) continue;
    const interestSaved = interestSavedAgainst(baseline, candidate);
    oneOffExtraPaymentCurve.push({
      year,
      interestSaved,
      monthsSaved: monthsSavedAgainst(baseline, candidate),
      monthlyEmiReduction: Math.max(0, candidate.openingMonthlyEmi - candidate.endingMonthlyEmi),
      extraPaid,
    });
  }

  const recurrentSchedule = repaymentYearsFor(selected, construction.possessionMonth);
  const crossoverYear = recurrentSchedule
    .find((point) => point.principalPaid >= point.interestPaid)?.year ?? null;
  const firstYearImpact = oneOffExtraPaymentCurve[0]?.interestSaved ?? 0;
  const halfImpactYear = firstYearImpact > 0
    ? oneOffExtraPaymentCurve.find((point) => (
      point.year > 1 && point.interestSaved <= firstYearImpact / 2
    ))?.year ?? null
    : null;
  const cadenceStartCurve: CadenceStartPoint[] = [];
  for (let startYear = 1; startYear <= maximumRunYears; startYear += 1) {
    const candidate = buildLoanSchedule(inputs, {
      extraEmisPerYear,
      strategy,
      extraEmisStartYear: startYear,
    }, config);
    cadenceStartCurve.push({
      startYear,
      interestSaved: interestSavedAgainst(baseline, candidate),
      monthsSaved: monthsSavedAgainst(baseline, candidate),
      monthlyEmiReduction: Math.max(0, candidate.openingMonthlyEmi - candidate.endingMonthlyEmi),
      extraPaid: totalExtraPaid(candidate),
    });
  }
  const firstYearCadenceImpact = cadenceStartCurve[0]?.interestSaved ?? 0;
  const halfCadenceImpactStartYear = firstYearCadenceImpact > 0
    ? cadenceStartCurve.find((point) => (
      point.startYear > 1 && point.interestSaved <= firstYearCadenceImpact / 2
    ))?.startYear ?? null
    : null;
  const cadenceComparison = ([0, 1, 2, 3, 4, 6] as const).map((cadence) => {
    const candidate = cadence === extraEmisPerYear
      ? selected
      : buildLoanSchedule(inputs, { extraEmisPerYear: cadence, strategy }, config);
    return {
      extraEmisPerYear: cadence,
      interestSaved: interestSavedAgainst(baseline, candidate),
      monthsSaved: monthsSavedAgainst(baseline, candidate),
      endingMonthlyEmi: candidate.endingMonthlyEmi,
      totalExtraPaid: totalExtraPaid(candidate),
    };
  });
  const strategyComparison = (["finish_earlier", "lower_emi"] as const).map(
    (comparisonStrategy) => {
      const candidate = comparisonStrategy === strategy
        ? selected
        : buildLoanSchedule(inputs, {
          extraEmisPerYear,
          strategy: comparisonStrategy,
        }, config);
      return {
        strategy: comparisonStrategy,
        interestSaved: interestSavedAgainst(baseline, candidate),
        monthsSaved: monthsSavedAgainst(baseline, candidate),
        endingMonthlyEmi: candidate.endingMonthlyEmi,
        totalExtraPaid: totalExtraPaid(candidate),
      };
    },
  );
  const firstExtraIndex = selected.months.findIndex((month) => month.extraPaid > 0);
  const firstRecalculatedMonthlyEmi = firstExtraIndex >= 0
    ? selected.months[firstExtraIndex + 1]?.scheduledEmi ?? selected.endingMonthlyEmi
    : selected.openingMonthlyEmi;
  const markers = {
    crossoverYear,
    halfFirstYearImpactYear: halfImpactYear,
    halfCadenceImpactStartYear,
  };
  const prepaymentRun: PrepaymentRunPoint[] = oneOffExtraPaymentCurve.map((point) => ({
    ...point,
    throughYear: point.year,
    incrementalInterestSaved: point.interestSaved,
  }));

  return {
    status: selected.status,
    strategy,
    extraEmisPerYear,
    openingMonthlyEmi: selected.openingMonthlyEmi,
    endingMonthlyEmi: selected.endingMonthlyEmi,
    firstRecalculatedMonthlyEmi,
    annualPrepayment: selected.annualPrepayment,
    interestSaved: interestSavedAgainst(baseline, selected),
    monthsSaved: monthsSavedAgainst(baseline, selected),
    recurrentSchedule,
    oneOffExtraPaymentCurve,
    cadenceStartCurve,
    cadenceComparison,
    strategyComparison,
    markers,
    repaymentYears: recurrentSchedule,
    prepaymentRun,
    crossoverYear,
    halfImpactYear,
  };
}
