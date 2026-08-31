import assert from "node:assert/strict";
import test from "node:test";
import { buildLoanSchedule } from "../src/features/home-plan/financeEngine.ts";
import type { PlanInputs } from "../src/features/home-plan/model.ts";
import {
  aggregateRepaymentSchedule,
  repaymentScheduleCsv,
} from "../src/features/home-plan/repaymentSchedule.ts";

const INPUTS: PlanInputs = {
  propertyPriceLakh: 208,
  downPaymentPercent: 20,
  monthlyEmiThousands: 135,
  loanRate: 7.5,
  currentRentThousands: 55,
  equityReturn: 10,
  monthlySipThousands: 135,
  holdingPeriodYears: 20,
  purchaseYear: 0,
  construction: {
    state: "ready",
    asOfDate: "2026-08-30",
    dateSource: "not_applicable",
  },
  assumptions: {
    homeAppreciationRate: 5,
    rentInflationRate: 5,
  },
};

function schedule(extraEmisPerYear: number) {
  const baseline = buildLoanSchedule(INPUTS, {
    extraEmisPerYear: 0,
  }).months.filter((month) => month.paymentNumber > 0);
  const selected = buildLoanSchedule(INPUTS, {
    extraEmisPerYear,
  }).months.filter((month) => month.paymentNumber > 0);
  return {
    selected,
    years: aggregateRepaymentSchedule(selected, baseline),
  };
}

test("yearly repayment values exactly aggregate their monthly rows", () => {
  const { years } = schedule(4);
  for (const year of years) {
    const sum = (pick: (month: typeof year.months[number]) => number) =>
      year.months.reduce((total, month) => total + pick(month), 0);
    assert.ok(Math.abs(year.totalPaid - sum((month) => month.totalPaid)) < 0.01);
    assert.ok(Math.abs(year.interestPaid - sum((month) => month.interestPaid)) < 0.01);
    assert.ok(Math.abs(year.principalPaid - sum((month) => month.principalPaid)) < 0.01);
    assert.ok(Math.abs(year.extraPaid - sum((month) => month.extraPaid)) < 0.01);
    assert.equal(year.closingBalance, year.months.at(-1)?.closingBalance);
  }
});

test("scheduled payment contains interest and scheduled principal without double-counting extras", () => {
  const { selected, years } = schedule(4);
  for (const month of selected) {
    assert.ok(
      Math.abs(month.scheduledPayment - month.interestPaid - month.principalPaid) < 0.01,
    );
  }
  const totalFromMonths = selected.reduce(
    (total, month) => total + month.scheduledPayment + month.extraPaid,
    0,
  );
  assert.ok(Math.abs(totalFromMonths - years.reduce((total, year) => total + year.totalPaid, 0)) < 0.01);
  assert.equal(years.at(-1)?.closingBalance, 0);
});

test("CSV annual totals match the visible yearly schedule", () => {
  const { years } = schedule(4);
  const csv = repaymentScheduleCsv(years);
  const firstYear = years[0];
  assert.ok(firstYear);
  assert.match(csv, /period,year,scheduled payment,interest,scheduled principal,extra principal,closing balance,cumulative interest avoided/);
  assert.ok(csv.includes([
    "Year 1",
    "1",
    (firstYear.totalPaid - firstYear.extraPaid).toFixed(2),
    firstYear.interestPaid.toFixed(2),
    firstYear.principalPaid.toFixed(2),
    firstYear.extraPaid.toFixed(2),
    firstYear.closingBalance.toFixed(2),
    firstYear.cumulativeInterestAvoided.toFixed(2),
  ].join(",")));
});
