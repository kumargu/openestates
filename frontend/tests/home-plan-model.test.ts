import assert from "node:assert/strict";
import test from "node:test";
import {
  BASE_INPUTS,
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
  calculateUpfrontCash,
  maximumDownPaymentLakh,
  minimumDownPaymentLakh,
} from "../src/features/home-plan/model.ts";
import { isExplicitlyReadyStatus } from "../src/features/home-plan/financeEngine.ts";

const readyInputs = {
  ...BASE_INPUTS,
  holdingPeriodYears: 20,
  construction: {
    state: "ready" as const,
    asOfDate: "2026-01-01",
    dateSource: "not_applicable" as const,
  },
};

const underConstructionInputs = {
  ...readyInputs,
  construction: {
    state: "under_construction" as const,
    asOfDate: "2026-01-01",
    startDate: "2025-01-01",
    completionDate: "2028-01-01",
    dateSource: "rera" as const,
  },
};

test("a larger down payment lowers EMI and interest while changing the final comparison", () => {
  const lowerDown = calculateProjection({ ...readyInputs, downPaymentLakh: 30 });
  const higherDown = calculateProjection({ ...readyInputs, downPaymentLakh: 45 });
  const lowerGap = lowerDown.points.at(-1)!.buyNetWorth - lowerDown.points.at(-1)!.rentNetWorth;
  const higherGap = higherDown.points.at(-1)!.buyNetWorth - higherDown.points.at(-1)!.rentNetWorth;

  assert.ok(higherDown.monthlyEmi < lowerDown.monthlyEmi);
  assert.ok(higherDown.totalInterest < lowerDown.totalInterest);
  assert.ok(Math.abs(higherGap - lowerGap) > 100_000);
});

test("a larger down payment improves the buy outcome when investment returns trail the loan rate", () => {
  const lowerDown = calculateProjection({ ...readyInputs, downPaymentLakh: 30, equityReturn: 6 });
  const higherDown = calculateProjection({ ...readyInputs, downPaymentLakh: 45, equityReturn: 6 });
  const lowerGap = lowerDown.points.at(-1)!.buyNetWorth - lowerDown.points.at(-1)!.rentNetWorth;
  const higherGap = higherDown.points.at(-1)!.buyNetWorth - higherDown.points.at(-1)!.rentNetWorth;

  assert.ok(higherGap > lowerGap);
});

test("changing financing does not create or destroy net worth on purchase day", () => {
  const lowerDown = calculateProjection({ ...readyInputs, downPaymentLakh: 30 });
  const higherDown = calculateProjection({ ...readyInputs, downPaymentLakh: 45 });

  assert.ok(Math.abs(lowerDown.points[0].buyNetWorth - higherDown.points[0].buyNetWorth) < 1);
});

test("a ready home is paid in one installment and starts EMI immediately", () => {
  const projection = calculateProjection(readyInputs);

  assert.equal(projection.paymentSchedule.length, 1);
  assert.equal(projection.paymentSchedule[0].month, 0);
  assert.equal(projection.paymentSchedule[0].amount, readyInputs.propertyPriceLakh * 100_000);
  assert.ok(projection.points[0].annualEmi > 0);
  assert.equal(projection.points[0].builderBalance, 0);
});

test("an under-construction home uses progress now and six-month installments", () => {
  const projection = calculateProjection(underConstructionInputs);
  const schedule = projection.paymentSchedule;
  const total = schedule.reduce((sum, payment) => sum + payment.amount, 0);

  assert.deepEqual(schedule.map((payment) => payment.month), [0, 6, 12, 18, 24]);
  assert.ok(Math.abs(schedule[0].amount - 5_000_000) < 1);
  assert.ok(Math.abs(total - underConstructionInputs.propertyPriceLakh * 100_000) < 1);
  assert.equal(projection.possessionMonth, 24);
  assert.equal(projection.constructionDateSource, "rera");
});

test("buyer keeps paying rent and pre-EMI interest until possession", () => {
  const projection = calculateProjection(underConstructionInputs);

  assert.equal(projection.points[0].annualEmi, 0);
  assert.ok(projection.points[0].monthlyBuyerHousingCost > projection.points[0].annualRent / 12);
  assert.ok(projection.points[2].annualEmi > 0);
});

test("missing construction dates use a clearly marked two-year estimate", () => {
  const inputs = buildBaselinePlanInputs(15_000_000, {
    state: "under_construction",
    asOfDate: "2026-01-01",
    dateSource: "estimated",
  });
  const projection = calculateProjection(inputs);

  assert.equal(projection.possessionMonth, 24);
  assert.equal(projection.possessionDate, "2028-01-01");
  assert.equal(projection.constructionDateSource, "estimated");
});

test("RERA day-first dates are normalized before building the schedule", () => {
  const projection = calculateProjection({
    ...underConstructionInputs,
    construction: {
      ...underConstructionInputs.construction,
      startDate: "01/01/2025",
      completionDate: "30/06/2028",
    },
  });

  assert.equal(projection.possessionDate, "2028-06-30");
  assert.equal(projection.constructionDateSource, "rera");
});

test("the default down payment never exceeds the stated savings", () => {
  const inputs = buildBaselinePlanInputs(30_000_000);

  assert.ok(inputs.downPaymentLakh <= maximumDownPaymentLakh(inputs));
  assert.ok(inputs.downPaymentLakh >= minimumDownPaymentLakh(inputs.propertyPriceLakh));
  assert.ok(calculateUpfrontCash(inputs) <= inputs.startingSavingsLakh * 100_000);
});

test("a project completed before a future purchase is paid as a ready home", () => {
  const projection = calculateProjection({
    ...underConstructionInputs,
    purchaseYear: 3,
    construction: {
      ...underConstructionInputs.construction,
      completionDate: "2028-01-01",
    },
  });

  assert.equal(projection.possessionMonth, 36);
  assert.equal(projection.paymentSchedule.length, 1);
  assert.equal(projection.paymentSchedule[0].month, 36);
});

test("the payoff journey uses staged draws and begins amortization at possession", () => {
  const projection = calculateProjection(underConstructionInputs);
  const journey = calculateLoanJourney(underConstructionInputs, 0);

  assert.equal(journey.points[0].balance, projection.paymentSchedule[0].loanAmount);
  assert.equal(journey.loanFreeMonths, projection.possessionMonth + underConstructionInputs.loanTenureYears * 12);
  assert.ok(Math.abs(journey.totalInterest - projection.totalInterest) < 1);
});

test("negated completion statuses are not treated as ready", () => {
  assert.equal(isExplicitlyReadyStatus("Completed"), true);
  assert.equal(isExplicitlyReadyStatus("Delivered · 1-5 yrs old"), true);
  assert.equal(isExplicitlyReadyStatus("Not Completed"), false);
  assert.equal(isExplicitlyReadyStatus("Under construction"), false);
});
