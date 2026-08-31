import type { LoanScheduleMonth } from "./financeEngine.ts";
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
}>;

function monthlyPoints(
  months: LoanScheduleMonth[],
): MonthlyRepaymentPoint[] {
  return months
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
  model: RepaymentDashboardModel,
): RepaymentChartStories {
  const baselineMonthly = monthlyPoints(model.baselineSchedule);
  const selectedMonthly = monthlyPoints(model.selectedSchedule);
  return {
    annual: model.recurrentSchedule,
    baselineMonthly,
    selectedMonthly,
  };
}
