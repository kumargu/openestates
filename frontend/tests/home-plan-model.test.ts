import assert from "node:assert/strict";
import test from "node:test";
import {
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
  hasPlannablePrice,
  type PlanInputs,
  updatePlanInput,
} from "../src/features/home-plan/model.ts";
import {
  DEFAULT_PLAN_MODEL_CONFIG,
  type PlanModelConfig,
} from "../src/features/home-plan/modelConfig.ts";
import { buildPlanSnapshotNote } from "../src/features/home-plan/planSnapshot.ts";
import { buildMonthlyPlanVerdict, defaultPlanFocusYear } from "../src/features/home-plan/monthlyPlanView.ts";
import {
  isExplicitlyReadyStatus,
  monthlyPayment,
  principalFromMonthlyPayment,
  rentInMonth,
} from "../src/features/home-plan/financeEngine.ts";

const BASE_INPUTS: PlanInputs = {
  propertyPriceLakh: 150,
  downPaymentPercent: DEFAULT_PLAN_MODEL_CONFIG.defaults.downPaymentPercent,
  monthlyEmiThousands: 90,
  loanRate: DEFAULT_PLAN_MODEL_CONFIG.defaults.loanRate,
  currentRentThousands: 55,
  equityReturn: DEFAULT_PLAN_MODEL_CONFIG.defaults.equityReturn,
  monthlySipThousands: 35,
  holdingPeriodYears: 15,
  purchaseYear: DEFAULT_PLAN_MODEL_CONFIG.defaults.purchaseYear,
  construction: {
    state: "ready",
    asOfDate: "2026-01-01",
    dateSource: "not_applicable",
  },
  assumptions: {
    homeAppreciationRate: DEFAULT_PLAN_MODEL_CONFIG.defaults.homeAppreciationRate,
    rentInflationRate: DEFAULT_PLAN_MODEL_CONFIG.defaults.rentInflationRate,
  },
};

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

test("buyer and renter start with matching down-payment capital", () => {
  const projection = calculateProjection(ready);
  const day0 = projection.points[0];
  const price = ready.propertyPriceLakh * 100_000;
  const downPayment = price * ready.downPaymentPercent / 100;

  assert.equal(projection.loanAmount, price - downPayment);
  assert.equal(projection.upfrontPayment, downPayment);
  assert.ok(Math.abs(day0.buyNetWorth - downPayment) < 1);
  assert.ok(Math.abs(day0.rentNetWorth - downPayment) < 1);
});

test("EMI and loan rate apply to the balance after down payment", () => {
  const projection = calculateProjection(ready);
  const price = ready.propertyPriceLakh * 100_000;

  assert.equal(projection.loanAmount, price * (1 - ready.downPaymentPercent / 100));
  assert.equal(projection.upfrontPayment, price * ready.downPaymentPercent / 100);
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
    propertyTitle: "Waterford Estate",
    inputs,
    projection,
    activeYear: 12,
  });

  assert.match(note.source, /^Saved \d{1,2} [A-Za-z]{3} \d{4}$/);
  assert.deepEqual(note.labels, ["finance", "emi", "price"]);
  assert.equal(note.title, "Waterford Estate plan, ₹1.8L EMI");
  assert.match(note.detail, /Waterford Estate/);
  assert.match(note.detail, /3 extra EMIs\/year/);
  assert.match(note.detail, /rent/i);
  assert.match(note.detail, /SIP/);
  assert.match(note.detail, /home.*projected near|home value reads near|home itself is projected near/i);
  assert.doesNotMatch(note.detail, /assuming|Assumptions:/i);
  assert.doesNotMatch(note.detail, /cash to close|down|planned loan/i);
  assert.equal(note.catalogKey, "plan:prop-one:current");
});

test("rent rises by the fixed yearly assumption", () => {
  const rentInflation = DEFAULT_PLAN_MODEL_CONFIG.defaults.rentInflationRate;
  assert.equal(rentInflation, 10);
  assert.equal(rentInMonth(55_000, rentInflation, 0), 55_000);
  assert.equal(rentInMonth(55_000, rentInflation, 120), 142_656);
});

test("baseline exposes monthly inputs", () => {
  const inputs = buildBaselinePlanInputs(15_000_000);
  const expectedEmi = Math.ceil(
    monthlyPayment(12_000_000, 7.5, 20) / 5_000,
  ) * 5;

  assert.equal(inputs.downPaymentPercent, 20);
  assert.equal(inputs.monthlyEmiThousands, expectedEmi);
  assert.ok(inputs.currentRentThousands > 0);
  assert.equal(
    inputs.monthlySipThousands + inputs.currentRentThousands,
    inputs.monthlyEmiThousands,
  );
  assert.equal(inputs.loanRate, 7.5);
  assert.equal(inputs.equityReturn, 10);
});

test("one model config drives baseline defaults and engine policy", () => {
  const config: PlanModelConfig = {
    defaults: {
      ...DEFAULT_PLAN_MODEL_CONFIG.defaults,
      downPaymentPercent: 30,
      loanRate: 9,
      loanTenureYears: 15,
      rentalYieldPercent: 4,
      equityReturn: 8,
      holdingPeriodYears: 10,
      extraEmisPerYear: 2,
      homeAppreciationRate: 5,
      rentInflationRate: 6,
    },
    construction: {
      ...DEFAULT_PLAN_MODEL_CONFIG.construction,
      paymentIntervalMonths: 3,
    },
    simulation: {
      ...DEFAULT_PLAN_MODEL_CONFIG.simulation,
      monthlyAmountStepThousands: 1,
    },
  };
  const inputs = buildBaselinePlanInputs(15_000_000, {
    state: "under_construction",
    asOfDate: "2026-01-01",
    startDate: "2025-01-01",
    completionDate: "2028-01-01",
    dateSource: "rera",
  }, config);
  const expectedEmi = Math.ceil(monthlyPayment(10_500_000, 9, 15) / 1_000);

  assert.equal(inputs.downPaymentPercent, 30);
  assert.equal(inputs.loanRate, 9);
  assert.equal(inputs.currentRentThousands, 50);
  assert.equal(inputs.monthlyEmiThousands, expectedEmi);
  assert.equal(inputs.equityReturn, 8);
  assert.equal(inputs.holdingPeriodYears, 10);
  assert.deepEqual(inputs.assumptions, {
    homeAppreciationRate: 5,
    rentInflationRate: 6,
  });
  const projection = calculateProjection(inputs, undefined, config);
  assert.equal(projection.extraEmisPerYear, 2);
  assert.deepEqual(
    projection.paymentSchedule.map((payment) => payment.month),
    [0, 3, 6, 9, 12, 15, 18, 21, 24],
  );
});

test("invalid model policy fails before simulation", () => {
  const invalidConfig: PlanModelConfig = {
    ...DEFAULT_PLAN_MODEL_CONFIG,
    construction: {
      ...DEFAULT_PLAN_MODEL_CONFIG.construction,
      paymentIntervalMonths: 0,
    },
  };

  assert.throws(
    () => buildBaselinePlanInputs(15_000_000, undefined, invalidConfig),
    /construction\.paymentIntervalMonths/,
  );
});

test("baseline EMI repays the price over the default tenure at any price", () => {
  for (const price of [6_700_000, 8_000_000, 15_100_000, 33_100_000, 90_000_000]) {
    const inputs = buildBaselinePlanInputs(price);
    const projection = calculateProjection(inputs);
    const loanPrincipal = price * (1 - inputs.downPaymentPercent / 100);
    const loanTenure = DEFAULT_PLAN_MODEL_CONFIG.defaults.loanTenureYears;
    const exactEmi = monthlyPayment(loanPrincipal, inputs.loanRate, loanTenure);

    assert.equal(projection.loanAmount, loanPrincipal);
    assert.equal(projection.upfrontPayment, price - loanPrincipal);
    // Rounded up to the nearest ₹5K step, so never below the amortizing EMI
    // and never more than one step above it.
    assert.ok(inputs.monthlyEmiThousands * 1_000 >= exactEmi);
    assert.ok(inputs.monthlyEmiThousands * 1_000 - exactEmi < 5_000);
    // A 20-year loan should read as a 20-year loan, not close in single digits.
    assert.ok(projection.loanFreeYear !== null);
    assert.ok(projection.loanFreeYear! > loanTenure - 3);
    assert.ok(projection.loanFreeYear! <= loanTenure);
  }
});

test("baseline rejects homes without a price instead of inventing one", () => {
  assert.equal(hasPlannablePrice(0), false);
  assert.equal(hasPlannablePrice(Number.NaN), false);
  assert.equal(hasPlannablePrice(6_700_000), true);
  assert.throws(() => buildBaselinePlanInputs(0), /propertyPriceInr/);
  assert.equal(buildBaselinePlanInputs(6_700_000).propertyPriceLakh, 67);
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

test("plan input edits stay independent except down payment recalculates EMI", () => {
  const higherRent = updatePlanInput(ready, "currentRentThousands", 65);
  const higherEmi = updatePlanInput(ready, "monthlyEmiThousands", 155);
  const higherDownPayment = updatePlanInput(ready, "downPaymentPercent", 30);
  const expectedEmi = Math.ceil(
    monthlyPayment(ready.propertyPriceLakh * 100_000 * 0.7, ready.loanRate, 20) / 5_000,
  ) * 5;

  assert.equal(higherRent.currentRentThousands, 65);
  assert.equal(higherRent.monthlySipThousands, ready.monthlySipThousands);
  assert.equal(higherEmi.monthlyEmiThousands, 155);
  assert.equal(higherEmi.monthlySipThousands, ready.monthlySipThousands);
  assert.equal(higherDownPayment.downPaymentPercent, 30);
  assert.equal(higherDownPayment.monthlyEmiThousands, expectedEmi);
  assert.equal(higherDownPayment.monthlySipThousands, ready.monthlySipThousands);
  assert.throws(() => updatePlanInput(ready, "monthlyEmiThousands", 0), /monthlyEmiThousands/);
  assert.throws(() => updatePlanInput(ready, "downPaymentPercent", 101), /downPaymentPercent/);
});

test("rent path compounds the stated SIP when rent holds steady", () => {
  const inputs = {
    ...ready,
    currentRentThousands: 35,
    monthlySipThousands: 40,
    equityReturn: 10,
    holdingPeriodYears: 20,
    assumptions: { ...ready.assumptions, rentInflationRate: 0 },
  };
  const projection = calculateProjection(inputs);
  const monthlyRate = inputs.equityReturn / 100 / 12;
  const months = inputs.holdingPeriodYears * 12;
  const matchingDownPayment = inputs.propertyPriceLakh * 100_000
    * inputs.downPaymentPercent / 100;
  const expectedSipValue = matchingDownPayment * (1 + monthlyRate) ** months
    + inputs.monthlySipThousands * 1_000
    * (((1 + monthlyRate) ** months - 1) / monthlyRate);

  assert.ok(Math.abs(projection.points.at(-1)!.rentNetWorth - expectedSipValue) < 10);
});

test("rising rent eats into the rent path's investing", () => {
  const inputs = {
    ...ready,
    currentRentThousands: 35,
    monthlySipThousands: 40,
    holdingPeriodYears: 20,
  };
  const steadyRent = calculateProjection({
    ...inputs,
    assumptions: { ...inputs.assumptions, rentInflationRate: 0 },
  });
  const risingRent = calculateProjection(inputs);
  const higherStartingRent = calculateProjection({ ...inputs, currentRentThousands: 80 });

  // The renter commits rent + SIP; rent rises, so less of it reaches the SIP.
  assert.ok(risingRent.points.at(-1)!.rentNetWorth < steadyRent.points.at(-1)!.rentNetWorth);
  // A costlier rental leaves less to invest out of the same commitment.
  assert.ok(higherStartingRent.points.at(-1)!.rentNetWorth < risingRent.points.at(-1)!.rentNetWorth);
});

test("break-even is reported only when buying actually overtakes renting", () => {
  const buyAhead = calculateProjection(buildBaselinePlanInputs(15_100_000));
  assert.ok(buyAhead.points[1].buyNetWorth >= buyAhead.points[1].rentNetWorth);
  assert.equal(buyAhead.breakEvenYear, null);

  // A large SIP puts renting ahead first, so a real crossover exists.
  const rentAheadFirst = calculateProjection({
    ...ready,
    monthlyEmiThousands: 135,
    monthlySipThousands: 400,
    holdingPeriodYears: 20,
  });
  const crossover = rentAheadFirst.breakEvenYear;
  assert.ok(crossover === null || rentAheadFirst.points[crossover - 1].buyNetWorth
    < rentAheadFirst.points[crossover - 1].rentNetWorth);
});

test("a closed loan frees the EMI into the buyer's wealth", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 220,
    holdingPeriodYears: 20,
  };
  const projection = calculateProjection(inputs);
  const loanFreeYear = projection.loanFreeYear!;
  const atPayoff = projection.points[loanFreeYear];
  const later = projection.points.at(-1)!;

  assert.ok(loanFreeYear < inputs.holdingPeriodYears);
  // Past payoff the buyer's wealth must outgrow the home's appreciation alone.
  const homeGrowth = later.propertyValue - atPayoff.propertyValue;
  assert.ok(later.buyNetWorth - atPayoff.buyNetWorth > homeGrowth);
});

test("home value uses the fixed six percent yearly growth assumption", () => {
  const projection = calculateProjection(ready);
  const end = projection.points.at(-1)!;
  const homeAppreciation = DEFAULT_PLAN_MODEL_CONFIG.defaults.homeAppreciationRate;
  const expected = ready.propertyPriceLakh * 100_000
    * (1 + homeAppreciation / 100 / 12) ** (ready.holdingPeriodYears * 12);

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
  assert.ok(projection.points[0].rentNetWorth > 0);
  assert.ok(Math.abs(
    projection.points[0].rentNetWorth - projection.points[0].buyNetWorth,
  ) < 1);
  assert.equal(
    projection.upfrontPayment,
    ready.propertyPriceLakh * 100_000 * ready.downPaymentPercent / 100,
  );
  for (const payment of projection.paymentSchedule) {
    assert.ok(Math.abs(payment.cashAmount - payment.amount * 0.2) < 1);
    assert.ok(Math.abs(payment.loanAmount - payment.amount * 0.8) < 1);
  }
});

test("payoff journey matches financing interest for a ready home", () => {
  const projection = calculateProjection(ready);
  const journey = calculateLoanJourney(ready, 0);

  assert.notEqual(journey.totalInterest, null);
  assert.notEqual(projection.totalInterest, null);
  assert.ok(Math.abs(journey.monthlyEmi - projection.monthlyEmi) < 1);
  assert.ok(Math.abs(journey.totalInterest! - projection.totalInterest!) < 1);
});

test("construction-stage draws and extra EMIs keep the graph balance aligned with payoff math", () => {
  const inputs = {
    ...ready,
    holdingPeriodYears: 20,
    construction: {
      state: "under_construction" as const,
      asOfDate: "2026-01-01",
      startDate: "2025-01-01",
      completionDate: "2028-01-01",
      dateSource: "rera" as const,
    },
  };
  const projection = calculateProjection(inputs, 3);
  const journey = calculateLoanJourney(inputs, 3);

  for (const point of projection.points) {
    const payoffPoint = journey.points.find((candidate) => candidate.year === point.year);
    if (payoffPoint) {
      assert.ok(Math.abs(point.loanBalance - payoffPoint.balance) < 1);
    }
  }
  assert.equal(projection.loanFreeYear, Math.ceil(journey.loanFreeMonths / 12));
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
    propertyTitle: "Waterford Estate",
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
  assert.match(view.insight, /At 12 years, (buying|renting) leads by ₹/);
  assert.match(view.insight, /3 extra EMIs\/year closes the loan/);
  assert.match(view.insight, /Total interest lands near/);
  assert.match(note.detail, /3 extra EMIs\/year/);
  assert.equal(note.catalogKey, "plan:home-1:current");
});

test("default plan focus stays on the selected holding period", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 20,
  };
  const projection = calculateProjection(inputs, 4);
  const focusYear = defaultPlanFocusYear(projection, inputs.holdingPeriodYears);
  const view = buildMonthlyPlanVerdict(projection, focusYear);

  assert.equal(focusYear, inputs.holdingPeriodYears);
  assert.match(view.timeLabel, new RegExp(`After ${inputs.holdingPeriodYears} years`));
  assert.match(view.insight, /4 extra EMIs\/year closes the loan/);
});

test("default plan focus falls back to the graph horizon when payoff is outside the chart", () => {
  const inputs = {
    ...ready,
    monthlyEmiThousands: 160,
    holdingPeriodYears: 5,
  };
  const projection = calculateProjection(inputs);

  assert.ok(projection.loanFreeYear !== null);
  assert.ok(projection.loanFreeYear! > inputs.holdingPeriodYears);
  assert.equal(defaultPlanFocusYear(projection, inputs.holdingPeriodYears), inputs.holdingPeriodYears);
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
  assert.match(view.insight, /At 10 years, (buying|renting) leads by ₹/);
  assert.match(view.insight, /loan does not close at this EMI/);
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
    propertyTitle: "Waterford Estate",
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
    propertyTitle: "Waterford Estate",
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
  assert.throws(() => calculateProjection(ready, Number.NaN), /extraEmisPerYear/);
  assert.throws(() => calculateProjection(ready, 1.5), /extraEmisPerYear/);
  assert.throws(() => calculateProjection({
    ...ready,
    downPaymentPercent: 101,
  }), /downPaymentPercent/);
});

test("a fully cash purchase has no loan or EMI", () => {
  const inputs = updatePlanInput(ready, "downPaymentPercent", 100);
  const projection = calculateProjection(inputs);

  assert.equal(inputs.monthlyEmiThousands, 0);
  assert.equal(projection.loanAmount, 0);
  assert.equal(projection.upfrontPayment, ready.propertyPriceLakh * 100_000);
  assert.equal(projection.loanFreeYear, 0);
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
