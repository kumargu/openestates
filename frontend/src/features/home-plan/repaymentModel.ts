import {
  buildLoanSchedule,
  constructionPlanFor,
  type LoanSchedule,
  type LoanScheduleMonth,
  type LoanRepaymentStatus,
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
  extraPaid: number;
};

export type RepaymentMarkers = {
  crossoverYear: number | null;
  halfFirstYearImpactYear: number | null;
};

export type RepaymentDashboardModel = {
  status: LoanRepaymentStatus;
  extraEmisPerYear: number;
  openingMonthlyEmi: number;
  interestSaved: number;
  monthsSaved: number;
  comparisonAvailable: boolean;
  baselinePayoffMonths: number | null;
  selectedPayoffMonths: number | null;
  baselineHorizonMonths: number;
  baselineSchedule: LoanScheduleMonth[];
  selectedSchedule: LoanScheduleMonth[];
  recurrentSchedule: RepaymentYearPoint[];
  oneOffExtraPaymentCurve: OneOffExtraPaymentPoint[];
  markers: RepaymentMarkers;
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
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): RepaymentDashboardModel {
  const baseline = buildLoanSchedule(inputs, { extraEmisPerYear: 0 }, config);
  const selected = buildLoanSchedule(inputs, { extraEmisPerYear }, config);
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
  for (let year = 1; baseline.totalInterest != null && year <= maximumRunYears; year += 1) {
    const candidate = buildLoanSchedule(inputs, {
      extraEmisPerYear: 1,
      oneOffExtraPaymentYear: year,
    }, config);
    const extraPaid = totalExtraPaid(candidate);
    if (extraPaid <= 0) continue;
    const interestSaved = interestSavedAgainst(baseline, candidate);
    oneOffExtraPaymentCurve.push({
      year,
      interestSaved,
      monthsSaved: monthsSavedAgainst(baseline, candidate),
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
  const markers = {
    crossoverYear,
    halfFirstYearImpactYear: halfImpactYear,
  };

  return {
    status: selected.status,
    extraEmisPerYear,
    openingMonthlyEmi: selected.openingMonthlyEmi,
    interestSaved: interestSavedAgainst(baseline, selected),
    monthsSaved: monthsSavedAgainst(baseline, selected),
    comparisonAvailable: baseline.payoffMonth != null && baseline.totalInterest != null,
    baselinePayoffMonths: baseline.payoffMonth == null
      ? null
      : baseline.months.at(-1)?.paymentNumber ?? null,
    selectedPayoffMonths: selected.payoffMonth == null
      ? null
      : selected.months.at(-1)?.paymentNumber ?? null,
    baselineHorizonMonths: baseline.months.at(-1)?.paymentNumber
      ?? config.simulation.maximumJourneyYears * MONTHS_IN_YEAR,
    baselineSchedule: baseline.months,
    selectedSchedule: selected.months,
    recurrentSchedule,
    oneOffExtraPaymentCurve,
    markers,
  };
}
