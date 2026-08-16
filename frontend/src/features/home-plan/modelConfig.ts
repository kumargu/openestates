/**
 * Defaults and engine policy for one rent-vs-buy model.
 *
 * `PlanInputs` remains the concrete property and buyer scenario. This config
 * contains only assumptions used to create or simulate that scenario, so a
 * new model can be exercised without changing finance-engine constants.
 */
export type PlanModelConfig = Readonly<{
  defaults: Readonly<{
    downPaymentPercent: number;
    loanRate: number;
    loanTenureYears: number;
    rentalYieldPercent: number;
    equityReturn: number;
    holdingPeriodYears: number;
    purchaseYear: number;
    extraEmisPerYear: number;
    homeAppreciationRate: number;
    rentInflationRate: number;
  }>;
  construction: Readonly<{
    paymentIntervalMonths: number;
    estimatedRemainingMonths: number;
    estimatedTotalMonths: number;
    minimumBookingPercent: number;
  }>;
  simulation: Readonly<{
    monthlyAmountStepThousands: number;
    maximumLoanYears: number;
    maximumJourneyYears: number;
    closedBalanceRupees: number;
  }>;
}>;

export const DEFAULT_PLAN_MODEL_CONFIG: PlanModelConfig = validatePlanModelConfig({
  defaults: {
    downPaymentPercent: 20,
    loanRate: 7.5,
    loanTenureYears: 20,
    rentalYieldPercent: 3.2,
    equityReturn: 10,
    holdingPeriodYears: 20,
    purchaseYear: 0,
    extraEmisPerYear: 0,
    homeAppreciationRate: 6,
    rentInflationRate: 10,
  },
  construction: {
    paymentIntervalMonths: 6,
    estimatedRemainingMonths: 24,
    estimatedTotalMonths: 36,
    minimumBookingPercent: 10,
  },
  simulation: {
    monthlyAmountStepThousands: 5,
    maximumLoanYears: 60,
    maximumJourneyYears: 40,
    closedBalanceRupees: 0.5,
  },
});

function requireFiniteAtLeast(value: number, field: string, minimum: number): void {
  if (!Number.isFinite(value) || value < minimum) {
    throw new RangeError(`${field} must be a finite number >= ${minimum}`);
  }
}

function requireWholeNumber(value: number, field: string, minimum: number): void {
  requireFiniteAtLeast(value, field, minimum);
  if (!Number.isInteger(value)) {
    throw new RangeError(`${field} must be a whole number`);
  }
}

function requirePercent(value: number, field: string): void {
  requireFiniteAtLeast(value, field, 0);
  if (value > 100) {
    throw new RangeError(`${field} must be between 0 and 100`);
  }
}

/** Protects configurable loops and formulas from impossible model policy. */
export function validatePlanModelConfig(config: PlanModelConfig): PlanModelConfig {
  const { defaults, construction, simulation } = config;
  requirePercent(defaults.downPaymentPercent, "defaults.downPaymentPercent");
  requireFiniteAtLeast(defaults.loanRate, "defaults.loanRate", 0);
  requireFiniteAtLeast(defaults.loanTenureYears, "defaults.loanTenureYears", 0.01);
  requireFiniteAtLeast(defaults.rentalYieldPercent, "defaults.rentalYieldPercent", 0);
  requireFiniteAtLeast(defaults.equityReturn, "defaults.equityReturn", 0);
  requireWholeNumber(defaults.holdingPeriodYears, "defaults.holdingPeriodYears", 0);
  requireFiniteAtLeast(defaults.purchaseYear, "defaults.purchaseYear", 0);
  requireWholeNumber(defaults.extraEmisPerYear, "defaults.extraEmisPerYear", 0);
  requireFiniteAtLeast(defaults.homeAppreciationRate, "defaults.homeAppreciationRate", 0);
  requireFiniteAtLeast(defaults.rentInflationRate, "defaults.rentInflationRate", 0);

  requireWholeNumber(construction.paymentIntervalMonths, "construction.paymentIntervalMonths", 1);
  requireWholeNumber(construction.estimatedRemainingMonths, "construction.estimatedRemainingMonths", 1);
  requireWholeNumber(construction.estimatedTotalMonths, "construction.estimatedTotalMonths", 1);
  requirePercent(construction.minimumBookingPercent, "construction.minimumBookingPercent");
  if (construction.estimatedTotalMonths < construction.estimatedRemainingMonths) {
    throw new RangeError(
      "construction.estimatedTotalMonths must cover estimatedRemainingMonths",
    );
  }

  requireFiniteAtLeast(
    simulation.monthlyAmountStepThousands,
    "simulation.monthlyAmountStepThousands",
    0.001,
  );
  requireWholeNumber(simulation.maximumLoanYears, "simulation.maximumLoanYears", 1);
  requireWholeNumber(simulation.maximumJourneyYears, "simulation.maximumJourneyYears", 1);
  requireFiniteAtLeast(simulation.closedBalanceRupees, "simulation.closedBalanceRupees", 0);
  return config;
}
