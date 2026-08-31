import type { MonthlyRepaymentPoint } from "./chartStories.ts";

export type RepaymentScheduleMonth = MonthlyRepaymentPoint & {
  totalPaid: number;
  cumulativeInterestAvoided: number;
};

export type RepaymentScheduleYear = {
  year: number;
  totalPaid: number;
  interestPaid: number;
  principalPaid: number;
  extraPaid: number;
  closingBalance: number;
  cumulativeInterestAvoided: number;
  months: RepaymentScheduleMonth[];
};

function cumulativeInterestByPayment(
  months: MonthlyRepaymentPoint[],
): Map<number, number> {
  const result = new Map<number, number>();
  let cumulative = 0;
  for (const month of months) {
    cumulative += month.interestPaid;
    result.set(month.paymentNumber, cumulative);
  }
  return result;
}

export function aggregateRepaymentSchedule(
  selected: MonthlyRepaymentPoint[],
  baseline: MonthlyRepaymentPoint[],
): RepaymentScheduleYear[] {
  const baselineInterest = cumulativeInterestByPayment(baseline);
  let selectedCumulativeInterest = 0;
  const years = new Map<number, RepaymentScheduleYear>();

  for (const month of selected) {
    selectedCumulativeInterest += month.interestPaid;
    const year = Math.ceil(month.paymentNumber / 12);
    const cumulativeInterestAvoided = Math.max(
      0,
      (baselineInterest.get(month.paymentNumber) ?? selectedCumulativeInterest)
        - selectedCumulativeInterest,
    );
    const scheduleMonth: RepaymentScheduleMonth = {
      ...month,
      totalPaid: month.scheduledPayment + month.extraPaid,
      cumulativeInterestAvoided,
    };
    const aggregate = years.get(year) ?? {
      year,
      totalPaid: 0,
      interestPaid: 0,
      principalPaid: 0,
      extraPaid: 0,
      closingBalance: month.closingBalance,
      cumulativeInterestAvoided: 0,
      months: [],
    };
    aggregate.totalPaid += scheduleMonth.totalPaid;
    aggregate.interestPaid += month.interestPaid;
    aggregate.principalPaid += month.principalPaid;
    aggregate.extraPaid += month.extraPaid;
    aggregate.closingBalance = month.closingBalance;
    aggregate.cumulativeInterestAvoided = cumulativeInterestAvoided;
    aggregate.months.push(scheduleMonth);
    years.set(year, aggregate);
  }

  return [...years.values()];
}

function csvNumber(value: number): string {
  return value.toFixed(2);
}

export function repaymentScheduleCsv(
  years: RepaymentScheduleYear[],
): string {
  const rows = [
    [
      "period",
      "year",
      "scheduled payment",
      "interest",
      "scheduled principal",
      "extra principal",
      "closing balance",
      "cumulative interest avoided",
    ].join(","),
  ];

  for (const year of years) {
    rows.push([
      `Year ${year.year}`,
      year.year,
      csvNumber(year.totalPaid - year.extraPaid),
      csvNumber(year.interestPaid),
      csvNumber(year.principalPaid),
      csvNumber(year.extraPaid),
      csvNumber(year.closingBalance),
      csvNumber(year.cumulativeInterestAvoided),
    ].join(","));
    for (const month of year.months) {
      rows.push([
        `Month ${month.paymentNumber}`,
        year.year,
        csvNumber(month.scheduledPayment),
        csvNumber(month.interestPaid),
        csvNumber(month.principalPaid),
        csvNumber(month.extraPaid),
        csvNumber(month.closingBalance),
        csvNumber(month.cumulativeInterestAvoided),
      ].join(","));
    }
  }

  return rows.join("\n");
}
