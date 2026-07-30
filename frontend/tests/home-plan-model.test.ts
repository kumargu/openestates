import assert from "node:assert/strict";
import test from "node:test";
import {
  BASE_INPUTS,
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
} from "../src/features/home-plan/model.ts";
import { buildPlanSnapshotNote } from "../src/features/home-plan/planSnapshot.ts";
import { buildMonthlyPlanVerdict } from "../src/features/home-plan/monthlyPlanView.ts";
import {
  FIXED_HOME_GROWTH_RATE,
  FIXED_RENT_INFLATION_RATE,
  isExplicitlyReadyStatus,
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

test("zero EMI is rejected at the algorithm boundary", () => {
  assert.throws(() => calculateProjection({
    ...ready,
    monthlyEmiThousands: 0,
  }), /monthlyEmiThousands must be a finite number >= 1/);
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

test("plan snapshot captures monthly assumptions and inspected outcome", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 180,
    currentRentThousands: 55,
    monthlySipThousands: 40,
    holdingPeriodYears: 20,
  };
  const projection = calculateProjection(inputs, 3);
  const note = buildPlanSnapshotNote({
    propertyId: "prop-one",
    inputs,
    projection,
    activeYear: 12,
  });

  assert.match(note.source, /^Saved \d{1,2} [A-Za-z]{3} \d{4}$/);
  assert.deepEqual(note.labels, ["finance", "emi", "price"]);
  assert.match(note.title, /^₹1\.8L EMI, loan closes in/);
  assert.match(note.detail, /Monthly plan:/);
  assert.match(note.detail, /3 extra EMIs\/year/);
  assert.match(note.detail, /Rent path:/);
  assert.match(note.detail, /Assumptions:/);
  assert.doesNotMatch(note.detail, /cash to close|down|planned loan/i);
  assert.equal(note.catalogKey, "plan:prop-one:current");
});

test("rent rises by the fixed yearly assumption", () => {
  assert.equal(FIXED_RENT_INFLATION_RATE, 10);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 0), 55_000);
  assert.equal(rentInMonth(55_000, FIXED_RENT_INFLATION_RATE, 120), 142_656);
});

test("baseline exposes monthly inputs", () => {
  const inputs = buildBaselinePlanInputs(15_000_000);
  const expectedEmi = Math.ceil(
    monthlyPayment(15_000_000, 7.5, 20) / 5_000,
  ) * 5;

  assert.equal(inputs.monthlyEmiThousands, expectedEmi);
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
  const projection = calculateProjection(inputs);

  assert.equal(
    inputs.monthlySipThousands + inputs.currentRentThousands,
    inputs.monthlyEmiThousands,
  );
  assert.ok(inputs.monthlySipThousands > 0);
  assert.ok(inputs.monthlyEmiThousands > inputs.currentRentThousands);
  assert.ok(projection.loanFreeYear !== null);
});

test("high-price EMI changes move the loan-free marker", () => {
  const baseline = buildBaselinePlanInputs(33_100_000);
  const lower = calculateProjection({
    ...baseline,
    holdingPeriodYears: 20,
  });
  const higher = calculateProjection({
    ...baseline,
    monthlyEmiThousands: baseline.monthlyEmiThousands + 80,
    holdingPeriodYears: 20,
  });

  assert.ok(lower.loanFreeYear !== null);
  assert.ok(higher.loanFreeYear !== null);
  assert.ok(higher.loanFreeYear! < lower.loanFreeYear!);
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
  assert.ok(Math.abs(projection.upfrontPayment) < 1);
});

test("payoff journey matches financing interest for a ready home", () => {
  const projection = calculateProjection(ready);
  const journey = calculateLoanJourney(ready, 0);

  assert.notEqual(journey.totalInterest, null);
  assert.notEqual(projection.totalInterest, null);
  assert.ok(Math.abs(journey.monthlyEmi - projection.monthlyEmi) < 1);
  assert.ok(Math.abs(journey.totalInterest! - projection.totalInterest!) < 1);
});

test("extra EMIs update payoff, total interest, snapshot, and top insight together", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 20,
  };
  const base = calculateProjection(inputs, 0);
  const prepaid = calculateProjection(inputs, 3);
  const view = buildMonthlyPlanVerdict(prepaid, 12);
  const note = buildPlanSnapshotNote({
    propertyId: "home-1",
    inputs,
    projection: prepaid,
    activeYear: view.activeYear,
  });

  assert.notEqual(base.loanFreeYear, null);
  assert.notEqual(prepaid.loanFreeYear, null);
  assert.notEqual(base.totalInterest, null);
  assert.notEqual(prepaid.totalInterest, null);
  assert.ok(prepaid.loanFreeYear! < base.loanFreeYear!);
  assert.ok(prepaid.totalInterest! < base.totalInterest!);
  assert.match(view.insight, /3 extra EMIs\/year closes the loan/);
  assert.match(view.insight, /Total interest lands near/);
  assert.match(note.detail, /3 extra EMIs\/year/);
  assert.equal(note.catalogKey, "plan:home-1:current");
});

test("low EMI plan returns explicit non-closing state without fake interest", () => {
  const projection = calculateProjection({
    ...ready,
    monthlyEmiThousands: 10,
    holdingPeriodYears: 20,
  });
  const view = buildMonthlyPlanVerdict(projection, 10);

  assert.equal(projection.loanFreeYear, null);
  assert.equal(projection.totalInterest, null);
  assert.equal(view.insight, "Loan does not close at this EMI.");
  assert.ok(projection.points.at(-1)!.loanBalance > projection.loanAmount);
});

test("journey and graph balances both capitalize unpaid interest", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 10,
    holdingPeriodYears: 5,
  };
  const projection = calculateProjection(inputs);
  const journey = calculateLoanJourney(inputs, 0);

  assert.equal(projection.loanFreeYear, null);
  assert.equal(journey.totalInterest, null);
  assert.ok(projection.points[5].loanBalance > projection.points[0].loanBalance);
  assert.ok(journey.points[5].balance > journey.points[0].balance);
});

test("short graph horizon preserves actual payoff year", () => {
  const projection = calculateProjection({
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 5,
  });
  const view = buildMonthlyPlanVerdict(projection, 5);

  assert.notEqual(projection.loanFreeYear, null);
  assert.ok(projection.loanFreeYear! > projection.points.length - 1);
  assert.doesNotMatch(view.insight, /Loan does not close/);
});

test("typed assumptions drive projection values instead of graph copy constants", () => {
  const defaultProjection = calculateProjection(ready);
  const warmerProjection = calculateProjection({
    ...ready,
    assumptions: {
      homeAppreciationRate: 8,
      rentInflationRate: 4,
    },
  });

  assert.equal(warmerProjection.assumptions.homeAppreciationRate, 8);
  assert.equal(warmerProjection.assumptions.rentInflationRate, 4);
  assert.ok(warmerProjection.points.at(-1)!.propertyValue > defaultProjection.points.at(-1)!.propertyValue);
  assert.ok(warmerProjection.points.at(-1)!.annualRent < defaultProjection.points.at(-1)!.annualRent);
});

test("future purchase payment schedule uses typed appreciation", () => {
  const defaultProjection = calculateProjection({
    ...ready,
    purchaseYear: 2,
  });
  const warmerProjection = calculateProjection({
    ...ready,
    purchaseYear: 2,
    assumptions: {
      ...ready.assumptions,
      homeAppreciationRate: 9,
    },
  });

  assert.ok(warmerProjection.loanAmount > defaultProjection.loanAmount);
  assert.ok(warmerProjection.paymentSchedule[0].amount > defaultProjection.paymentSchedule[0].amount);
});

test("snapshot identity stays stable so saved notebook plans update in place", () => {
  const base = buildPlanSnapshotNote({
    propertyId: "home-1",
    inputs: ready,
    projection: calculateProjection(ready, 2),
    activeYear: 8,
  });
  const changedInputs = {
    ...ready,
    assumptions: {
      ...ready.assumptions,
      rentInflationRate: 4,
    },
    construction: {
      state: "under_construction" as const,
      asOfDate: "2026-01-01",
      completionDate: "2028-06-01",
      dateSource: "rera" as const,
    },
  };
  const changed = buildPlanSnapshotNote({
    propertyId: "home-1",
    inputs: changedInputs,
    projection: calculateProjection(changedInputs, 2),
    activeYear: 8,
  });

  assert.equal(base.catalogKey, changed.catalogKey);
  assert.notEqual(base.detail, changed.detail);
});

test("monthly plan rejects invalid numeric inputs at the algorithm boundary", () => {
  assert.throws(() => calculateProjection({
    ...ready,
    monthlyEmiThousands: Number.NaN,
  }), /monthlyEmiThousands/);
  assert.throws(() => calculateProjection({
    ...ready,
    assumptions: {
      ...ready.assumptions,
      rentInflationRate: Number.POSITIVE_INFINITY,
    },
  }), /rentInflationRate/);
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
