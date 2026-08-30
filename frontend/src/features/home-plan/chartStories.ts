import {
  buildLoanSchedule,
  type LoanScheduleMonth,
} from "./financeEngine.ts";
import type { PlanInputs } from "./model.ts";
import type {
  RepaymentDashboardModel,
  RepaymentYearPoint,
} from "./repaymentModel.ts";

export type MonthlyRepaymentPoint = Pick<
  LoanScheduleMonth,
  | "month"
  | "paymentNumber"
  | "scheduledEmi"
  | "scheduledPayment"
  | "interestPaid"
  | "principalPaid"
  | "extraPaid"
  | "closingBalance"
>;

export type RepaymentChartStories = Readonly<{
  annual: RepaymentYearPoint[];
  baselineMonthly: MonthlyRepaymentPoint[];
  selectedMonthly: MonthlyRepaymentPoint[];
  finishEarlierMonthly: MonthlyRepaymentPoint[];
  lowerEmiMonthly: MonthlyRepaymentPoint[];
  /** @deprecated Use `selectedMonthly`. */
  monthly: MonthlyRepaymentPoint[];
}>;

function monthlyPoints(
  inputs: PlanInputs,
  extraEmisPerYear: number,
  strategy: RepaymentDashboardModel["strategy"],
): MonthlyRepaymentPoint[] {
  return buildLoanSchedule(inputs, { extraEmisPerYear, strategy }).months
    .filter((month) => month.paymentNumber > 0)
    .map((month) => ({
      month: month.month,
      paymentNumber: month.paymentNumber,
      scheduledEmi: month.scheduledEmi,
      scheduledPayment: month.scheduledPayment,
      interestPaid: month.interestPaid,
      principalPaid: month.principalPaid,
      extraPaid: month.extraPaid,
      closingBalance: month.closingBalance,
    }));
}

export function buildRepaymentChartStories(
  inputs: PlanInputs,
  model: RepaymentDashboardModel,
): RepaymentChartStories {
  const baselineMonthly = monthlyPoints(inputs, 0, "finish_earlier");
  const selectedMonthly = monthlyPoints(inputs, model.extraEmisPerYear, model.strategy);
  const finishEarlierMonthly = model.strategy === "finish_earlier"
    ? selectedMonthly
    : monthlyPoints(inputs, model.extraEmisPerYear, "finish_earlier");
  const lowerEmiMonthly = model.strategy === "lower_emi"
    ? selectedMonthly
    : monthlyPoints(inputs, model.extraEmisPerYear, "lower_emi");
  return {
    annual: model.recurrentSchedule,
    baselineMonthly,
    selectedMonthly,
    finishEarlierMonthly,
    lowerEmiMonthly,
    monthly: selectedMonthly,
  };
}
