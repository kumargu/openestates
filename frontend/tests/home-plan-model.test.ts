import assert from "node:assert/strict";
import test from "node:test";
import {
  BASE_INPUTS,
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
} from "../src/features/home-plan/model.ts";
import {
  isExplicitlyReadyStatus,
  FIXED_HOME_GROWTH_RATE,
  FIXED_RENT_INFLATION_RATE,
  monthlyPayment,
  principalFromMonthlyPayment,
  rentInMonth,
} from "../src/features/home-plan/financeEngine.ts";

const ready = {
  ...BASE_INPUTS,
  propertyPriceLakh: 150,
  monthlyEmiThousands: 95,
  loanRate: 8.5,
  currentRentThousands: 55,
  equityReturn: 10,
  monthlySipThousands: 40,
  holdingPeriodYears: 10,
  purchaseYear: 0,
  construction: {
    state: "ready" as const,
    asOfDate: "2026-01-01",
    dateSource: "not_applicable" as const,
  },
};

test("day 0 wealth matches because both paths start with the implied upfront amount", () => {
  const projection = calculateProjection(ready);
  const day0 = projection.points[0];

  assert.ok(Math.abs(day0.buyNetWorth - day0.rentNetWorth) < 1);
  assert.ok(Math.abs(day0.buyNetWorth - projection.upfrontPayment) < 1);
});

test("monthly EMI and loan rate determine the loan and upfront payment", () => {
  const propertyPrice = ready.propertyPriceLakh * 100_000;
  const expectedLoan = principalFromMonthlyPayment(
    ready.monthlyEmiThousands * 1_000,
    ready.loanRate,
    20,
  );
  const projection = calculateProjection(ready);

  assert.ok(Math.abs(projection.loanAmount - expectedLoan) < 1);
  assert.ok(Math.abs(projection.upfrontPayment - (propertyPrice - expectedLoan)) < 1);
  assert.ok(Math.abs(projection.monthlyEmi - ready.monthlyEmiThousands * 1_000) < 1);
});

test("zero EMI means paying the full property price upfront", () => {
  const projection = calculateProjection({
    ...ready,
    monthlyEmiThousands: 0,
  });

  assert.equal(projection.monthlyEmi, 0);
  assert.equal(projection.loanAmount, 0);
  assert.equal(projection.upfrontPayment, ready.propertyPriceLakh * 100_000);
});

test("a larger EMI finances more of the home and lowers the upfront payment", () => {
  const lower = calculateProjection({ ...ready, monthlyEmiThousands: 60 });
  const higher = calculateProjection({ ...ready, monthlyEmiThousands: 100 });

  assert.ok(higher.loanAmount > lower.loanAmount);
  assert.ok(higher.upfrontPayment < lower.upfrontPayment);
});

test("an EMI above the 20-year amount pays a full-price loan off early", () => {
  const projection = calculateProjection({
    ...ready,
    monthlyEmiThousands: 300,
    holdingPeriodYears: 20,
  });
  const loanFreeYear = projection.points.find((point) => (
    point.year > 0 && point.loanBalance <= 0.5
  ))?.year;

  assert.equal(projection.loanAmount, ready.propertyPriceLakh * 100_000);
  assert.equal(projection.upfrontPayment, 0);
  assert.equal(projection.monthlyEmi, 300_000);
  assert.ok(loanFreeYear !== undefined && loanFreeYear < 20);
});

test("rent rise is fixed at zero in the simple plan", () => {
  assert.equal(FIXED_RENT_INFLATION_RATE, 0);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 0), 55_000);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 120), 55_000);
});

test("baseline exposes only the five editable money inputs", () => {
  const inputs = buildBaselinePlanInputs(15_000_000);

  assert.ok(inputs.monthlyEmiThousands > 0);
  assert.ok(inputs.currentRentThousands > 0);
  assert.ok(inputs.monthlySipThousands >= 0);
  assert.equal(inputs.loanRate, 8.5);
  assert.equal(inputs.equityReturn, 10);
});

test("monthly SIP grows only the rent and invest path", () => {
  const without = calculateProjection({ ...ready, monthlySipThousands: 0 });
  const withSip = calculateProjection({ ...ready, monthlySipThousands: 20 });
  const withoutEnd = without.points.at(-1)!;
  const withEnd = withSip.points.at(-1)!;

  assert.ok(withEnd.rentNetWorth > withoutEnd.rentNetWorth);
  assert.ok(Math.abs(withEnd.buyNetWorth - withoutEnd.buyNetWorth) < 1);
});

test("higher rent lowers the rent and invest projection", () => {
  const lowerRent = calculateProjection({ ...ready, currentRentThousands: 30 });
  const higherRent = calculateProjection({ ...ready, currentRentThousands: 80 });
  const lowerRentEnd = lowerRent.points.at(-1)!.rentNetWorth;
  const higherRentEnd = higherRent.points.at(-1)!.rentNetWorth;

  assert.equal(lowerRent.monthlyRent, 30_000);
  assert.equal(higherRent.monthlyRent, 80_000);
  assert.ok(lowerRentEnd > higherRentEnd);
  assert.ok(lowerRentEnd - higherRentEnd > 1_000_000);
});

test("home value uses the fixed six percent yearly growth assumption", () => {
  const projection = calculateProjection(ready);
  const end = projection.points.at(-1)!;
  const expected = ready.propertyPriceLakh * 100_000
    * (1 + FIXED_HOME_GROWTH_RATE / 100 / 12) ** (ready.holdingPeriodYears * 12);

  assert.ok(Math.abs(end.propertyValue - expected) < 10);
});

test("under-construction homes still stage builder payments every six months", () => {
  const projection = calculateProjection({
    ...ready,
    construction: {
      state: "under_construction",
      asOfDate: "2026-01-01",
      startDate: "2025-01-01",
      completionDate: "2028-01-01",
      dateSource: "rera",
    },
  });

  assert.deepEqual(projection.paymentSchedule.map((payment) => payment.month), [0, 6, 12, 18, 24]);
  assert.equal(projection.possessionMonth, 24);
  assert.equal(projection.points[0].annualEmi, 0);
  assert.ok(projection.points[2].annualEmi > 0);
  assert.ok(Math.abs(
    projection.points[0].buyNetWorth - projection.points[0].rentNetWorth,
  ) < 1);
});

test("payoff journey matches financing interest for a ready home", () => {
  const projection = calculateProjection(ready);
  const journey = calculateLoanJourney(ready, 0);

  assert.ok(Math.abs(journey.monthlyEmi - projection.monthlyEmi) < 1);
  assert.ok(Math.abs(journey.totalInterest - projection.totalInterest) < 1);
});

test("payment and principal formulas are inverses", () => {
  const principal = 11_000_000;
  const payment = monthlyPayment(principal, 8.5, 20);
  const restored = principalFromMonthlyPayment(payment, 8.5, 20);

  assert.ok(Math.abs(restored - principal) < 1);
});

test("negated completion statuses are not treated as ready", () => {
  assert.equal(isExplicitlyReadyStatus("Completed"), true);
  assert.equal(isExplicitlyReadyStatus("Not Completed"), false);
});
