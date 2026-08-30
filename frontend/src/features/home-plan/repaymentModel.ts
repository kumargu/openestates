import {
  buildLoanSchedule,
  constructionPlanFor,
  type LoanSchedule,
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
  interestPaid: number;
  principalPaid: number;
  extraPaid: number;
  balance: number;
};

export type PrepaymentRunPoint = {
  throughYear: number;
  interestSaved: number;
  incrementalInterestSaved: number;
  monthsSaved: number;
  monthlyEmiReduction: number;
};

export type RepaymentDashboardModel = {
  strategy: RepaymentStrategy;
  extraEmisPerYear: number;
  openingMonthlyEmi: number;
  endingMonthlyEmi: number;
  firstRecalculatedMonthlyEmi: number;
  annualPrepayment: number;
  interestSaved: number;
  monthsSaved: number;
  repaymentYears: RepaymentYearPoint[];
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
      interestPaid: 0,
      principalPaid: 0,
      extraPaid: 0,
      balance: month.openingBalance,
    };
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
      interestPaid: 0,
      principalPaid: 0,
      extraPaid: 0,
      balance: 0,
    }));
  }

  return [...byYear.values()];
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
  const prepaymentRun: PrepaymentRunPoint[] = [];
  let previousInterestSaved = 0;

  for (let throughYear = 0; throughYear <= maximumRunYears; throughYear += 1) {
    const candidate = throughYear === 0
      ? baseline
      : buildLoanSchedule(inputs, {
        extraEmisPerYear,
        strategy,
        extraPaymentsThroughYear: throughYear,
      }, config);
    const interestSaved = baseline.totalInterest != null && candidate.totalInterest != null
      ? Math.max(0, baseline.totalInterest - candidate.totalInterest)
      : 0;
    prepaymentRun.push({
      throughYear,
      interestSaved,
      incrementalInterestSaved: Math.max(0, interestSaved - previousInterestSaved),
      monthsSaved: baseline.payoffMonth != null && candidate.payoffMonth != null
        ? Math.max(0, baseline.payoffMonth - candidate.payoffMonth)
        : 0,
      monthlyEmiReduction: Math.max(0, candidate.openingMonthlyEmi - candidate.endingMonthlyEmi),
    });
    previousInterestSaved = interestSaved;
  }

  const repaymentYears = repaymentYearsFor(selected, construction.possessionMonth);
  const crossoverYear = repaymentYears.find((point) => point.principalPaid >= point.interestPaid)?.year ?? null;
  const firstYearImpact = prepaymentRun[1]?.incrementalInterestSaved ?? 0;
  const halfImpactYear = firstYearImpact > 0
    ? prepaymentRun.find((point) => (
      point.throughYear > 1
      && point.incrementalInterestSaved <= firstYearImpact / 2
    ))?.throughYear ?? null
    : null;

  return {
    strategy,
    extraEmisPerYear,
    openingMonthlyEmi: selected.openingMonthlyEmi,
    endingMonthlyEmi: selected.endingMonthlyEmi,
    firstRecalculatedMonthlyEmi: Math.max(
      0,
      selected.openingMonthlyEmi - (prepaymentRun[1]?.monthlyEmiReduction ?? 0),
    ),
    annualPrepayment: selected.annualPrepayment,
    interestSaved: baseline.totalInterest != null && selected.totalInterest != null
      ? Math.max(0, baseline.totalInterest - selected.totalInterest)
      : 0,
    monthsSaved: baseline.payoffMonth != null && selected.payoffMonth != null
      ? Math.max(0, baseline.payoffMonth - selected.payoffMonth)
      : 0,
    repaymentYears,
    prepaymentRun,
    crossoverYear,
    halfImpactYear,
  };
}
