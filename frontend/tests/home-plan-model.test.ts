import assert from "node:assert/strict";
import test from "node:test";
import {
  BASE_INPUTS,
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
} from "../src/features/home-plan/model.ts";
import { buildPlanSnapshotNote } from "../src/features/home-plan/planSnapshot.ts";
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
  monthlyEmiThousands: 135,
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

test("buyer starts fully financed with no invented down payment", () => {
  const projection = calculateProjection(ready);
  const day0 = projection.points[0];

  assert.equal(projection.loanAmount, ready.propertyPriceLakh * 100_000);
  assert.equal(projection.upfrontPayment, 0);
  assert.ok(Math.abs(day0.buyNetWorth) < 1);
  assert.equal(day0.rentNetWorth, 0);
});

test("EMI and loan rate keep the loan at the home price", () => {
  const projection = calculateProjection(ready);

  assert.equal(projection.loanAmount, ready.propertyPriceLakh * 100_000);
  assert.equal(projection.upfrontPayment, 0);
  assert.ok(Math.abs(projection.monthlyEmi - ready.monthlyEmiThousands * 1_000) < 1);
});

test("zero EMI means owning the home with no loan", () => {
  const projection = calculateProjection({
    ...ready,
    monthlyEmiThousands: 0,
  });

  assert.equal(projection.monthlyEmi, 0);
  assert.equal(projection.loanAmount, 0);
  assert.equal(projection.upfrontPayment, 0);
  assert.ok(Math.abs(projection.points[0].buyNetWorth - ready.propertyPriceLakh * 100_000) < 1);
});

test("higher EMI clears the loan sooner", () => {
  const lower = calculateProjection({
    ...ready,
    monthlyEmiThousands: 135,
    holdingPeriodYears: 20,
  });
  const higher = calculateProjection({
    ...ready,
    monthlyEmiThousands: 220,
    holdingPeriodYears: 20,
  });

  assert.ok(higher.loanFreeYear !== null);
  assert.ok(lower.loanFreeYear !== null);
  assert.ok(higher.loanFreeYear! < lower.loanFreeYear!);
  assert.ok(higher.points[10].buyNetWorth > lower.points[10].buyNetWorth);
});

test("higher loan rate delays the loan-free year", () => {
  const lower = calculateProjection({
    ...ready,
    loanRate: 6,
    monthlyEmiThousands: 180,
    holdingPeriodYears: 20,
  });
  const higher = calculateProjection({
    ...ready,
    loanRate: 11,
    monthlyEmiThousands: 180,
    holdingPeriodYears: 20,
  });

  assert.ok(lower.loanFreeYear !== null);
  assert.ok(higher.loanFreeYear !== null);
  assert.ok(lower.loanFreeYear! < higher.loanFreeYear!);
  assert.ok(lower.points[10].buyNetWorth > higher.points[10].buyNetWorth);
});

test("extra EMIs pull the loan-free marker forward", () => {
  const base = calculateProjection({
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 20,
  }, 0);
  const prepaid = calculateProjection({
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 20,
  }, 4);

  assert.ok(base.loanFreeYear !== null);
  assert.ok(prepaid.loanFreeYear !== null);
  assert.ok(prepaid.loanFreeYear! < base.loanFreeYear!);
});

test("plan snapshot captures assumptions and inspected outcome", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 180,
    currentRentThousands: 55,
    monthlySipThousands: 40,
    holdingPeriodYears: 20,
  };
  const projection = calculateProjection(inputs, 3);
  const activeYear = 12;
  const note = buildPlanSnapshotNote({
    propertyId: "prop-one",
    inputs,
    projection,
    activeYear,
    activePoint: projection.points[activeYear],
    extraEmisPerYear: 3,
  });

  assert.equal(note.source, "Plan snapshot");
  assert.deepEqual(note.labels, ["finance", "emi", "down-payment", "price"]);
  assert.match(note.title, /EMI, loan closes in/);
  assert.match(note.detail, /3 extra EMIs\/year/);
  assert.match(note.detail, /rent \+ .* SIP/);
  assert.match(note.detail, /home is projected at/);
  assert.match(note.catalogKey, /^plan:prop-one:12:/);
});

test("rent rises by the fixed yearly assumption", () => {
  assert.equal(FIXED_RENT_INFLATION_RATE, 10);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 0), 55_000);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 120), 142_656);
});

test("baseline exposes only the five editable money inputs", () => {
  const inputs = buildBaselinePlanInputs(15_000_000);

  assert.equal(inputs.monthlyEmiThousands, 90);
  assert.ok(inputs.currentRentThousands > 0);
  assert.equal(
    inputs.monthlySipThousands + inputs.currentRentThousands,
    inputs.monthlyEmiThousands,
  );
  assert.equal(inputs.loanRate, 7.5);
  assert.equal(inputs.equityReturn, 10);
});

test("high-price baseline keeps a visible SIP while preserving EMI equals rent plus SIP", () => {
  const inputs = buildBaselinePlanInputs(33_100_000);

  assert.equal(
    inputs.monthlySipThousands + inputs.currentRentThousands,
    inputs.monthlyEmiThousands,
  );
  assert.ok(inputs.monthlySipThousands > 0);
  assert.ok(inputs.monthlyEmiThousands > inputs.currentRentThousands);
});

test("monthly SIP grows only the rent and invest path", () => {
  const without = calculateProjection({ ...ready, monthlySipThousands: 0 });
  const withSip = calculateProjection({ ...ready, monthlySipThousands: 20 });
  const withoutEnd = without.points.at(-1)!;
  const withEnd = withSip.points.at(-1)!;

  assert.ok(withEnd.rentNetWorth > withoutEnd.rentNetWorth);
  assert.ok(Math.abs(withEnd.buyNetWorth - withoutEnd.buyNetWorth) < 1);
});

test("rent path contains only the stated SIP", () => {
  const inputs = {
    ...ready,
    currentRentThousands: 35,
    monthlySipThousands: 40,
    equityReturn: 10,
    holdingPeriodYears: 20,
  };
  const projection = calculateProjection(inputs);
  const monthlyRate = inputs.equityReturn / 100 / 12;
  const months = inputs.holdingPeriodYears * 12;
  const expectedSipValue = inputs.monthlySipThousands * 1_000
    * (((1 + monthlyRate) ** months - 1) / monthlyRate);

  assert.ok(Math.abs(projection.points.at(-1)!.rentNetWorth - expectedSipValue) < 10);

  const withDifferentRent = calculateProjection({
    ...inputs,
    currentRentThousands: 80,
  });
  assert.ok(
    Math.abs(
      withDifferentRent.points.at(-1)!.rentNetWorth
      - projection.points.at(-1)!.rentNetWorth,
    ) < 1,
  );
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
  assert.equal(projection.points[0].rentNetWorth, 0);
  assert.equal(projection.upfrontPayment, 0);
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
