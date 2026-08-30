import {
  buildLoanSchedule,
  buildPaymentSchedule,
  calculateProjectionPoints,
  constructionPlanFor,
  monthlyPayment,
  type RepaymentStrategy,
} from "./financeEngine.ts";
import {
  DEFAULT_PLAN_MODEL_CONFIG,
  type PlanModelConfig,
  validatePlanModelConfig,
} from "./modelConfig.ts";

export type ConstructionProfile = {
  state: "ready" | "under_construction";
  asOfDate: string;
  startDate?: string;
  completionDate?: string;
  dateSource: "rera" | "estimated" | "not_applicable";
};

export type PlanAssumptions = {
  homeAppreciationRate: number;
  rentInflationRate: number;
};

export type PlanInputs = {
  propertyPriceLakh: number;
  /** Share of each builder payment paid in cash rather than borrowed. */
  downPaymentPercent: number;
  /** EMI after possession. With the loan rate, it sets how fast the home is paid off. */
  monthlyEmiThousands: number;
  loanRate: number;
  currentRentThousands: number;
  equityReturn: number;
  /** Monthly investment on the rent path. */
  monthlySipThousands: number;
  holdingPeriodYears: number;
  purchaseYear: number;
  construction: ConstructionProfile;
  assumptions: PlanAssumptions;
};

export type EditablePlanInput =
  | "downPaymentPercent"
  | "monthlyEmiThousands"
  | "currentRentThousands"
  | "monthlySipThousands"
  | "loanRate"
  | "equityReturn";

export type ProjectionPoint = {
  year: number;
  buyNetWorth: number;
  rentNetWorth: number;
  propertyValue: number;
  loanBalance: number;
  builderBalance: number;
  annualRent: number;
  annualEmi: number;
  monthlyBuyerHousingCost: number;
};

export type PlanProjection = {
  monthlyEmi: number;
  monthlyRent: number;
  monthlySip: number;
  loanAmount: number;
  upfrontPayment: number;
  totalInterest: number | null;
  breakEvenYear: number | null;
  /** First year the loan is cleared by the monthly plan, even beyond the graph horizon. */
  loanFreeYear: number | null;
  extraEmisPerYear: number;
  repaymentStrategy: RepaymentStrategy;
  endingMonthlyEmi: number;
  possessionMonth: number;
  possessionDate: string | null;
  constructionDateSource: ConstructionProfile["dateSource"];
  assumptions: PlanAssumptions;
  paymentSchedule: BuilderPayment[];
  points: ProjectionPoint[];
};

export type BuilderPayment = {
  month: number;
  date: string;
  amount: number;
  cashAmount: number;
  loanAmount: number;
};

export type LoanJourneyPoint = {
  year: number;
  balance: number;
  interestPaid: number;
  principalPaid: number;
  extraPaid: number;
};

export type LoanJourney = {
  monthlyEmi: number;
  endingMonthlyEmi: number;
  annualPrepayment: number;
  loanFreeMonths: number;
  monthsSaved: number;
  interestSaved: number | null;
  totalInterest: number | null;
  repaymentStrategy: RepaymentStrategy;
  points: LoanJourneyPoint[];
};

const MONTHS_IN_YEAR = 12;
const LAKH = 100_000;

function finiteAmount(value: number, field: string, min = 0): number {
  if (!Number.isFinite(value) || value < min) {
    throw new RangeError(`${field} must be a finite number >= ${min}`);
  }
  return value;
}

function finitePercent(value: number, field: string): number {
  const percent = finiteAmount(value, field);
  if (percent > 100) {
    throw new RangeError(`${field} must be a finite number between 0 and 100`);
  }
  return percent;
}

function normalizeExtraEmisPerYear(value: number): number {
  if (!Number.isFinite(value) || value < 0 || !Number.isInteger(value)) {
    throw new RangeError("extraEmisPerYear must be a finite whole number >= 0");
  }
  return value;
}

export function normalizePlanInputs(inputs: PlanInputs): PlanInputs {
  const propertyPriceLakh = finiteAmount(inputs.propertyPriceLakh, "propertyPriceLakh", 0.01);
  const downPaymentPercent = finitePercent(inputs.downPaymentPercent, "downPaymentPercent");
  const minimumEmi = downPaymentPercent === 100 ? 0 : 1;
  return {
    ...inputs,
    propertyPriceLakh,
    downPaymentPercent,
    monthlyEmiThousands: finiteAmount(inputs.monthlyEmiThousands, "monthlyEmiThousands", minimumEmi),
    loanRate: finiteAmount(inputs.loanRate, "loanRate"),
    currentRentThousands: finiteAmount(inputs.currentRentThousands, "currentRentThousands"),
    equityReturn: finiteAmount(inputs.equityReturn, "equityReturn"),
    monthlySipThousands: finiteAmount(inputs.monthlySipThousands, "monthlySipThousands"),
    holdingPeriodYears: Math.max(0, Math.floor(finiteAmount(inputs.holdingPeriodYears, "holdingPeriodYears"))),
    purchaseYear: Math.max(0, finiteAmount(inputs.purchaseYear, "purchaseYear")),
    assumptions: {
      homeAppreciationRate: finiteAmount(inputs.assumptions.homeAppreciationRate, "homeAppreciationRate"),
      rentInflationRate: finiteAmount(inputs.assumptions.rentInflationRate, "rentInflationRate"),
    },
  };
}

/** Updates one buyer input; down payment also refreshes its configured-tenure EMI. */
export function updatePlanInput(
  inputs: PlanInputs,
  key: EditablePlanInput,
  value: number,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): PlanInputs {
  const minimum = key === "monthlyEmiThousands" && inputs.downPaymentPercent < 100 ? 1 : 0;
  const maximum = key === "downPaymentPercent" ? 100 : Number.POSITIVE_INFINITY;
  if (!Number.isFinite(value) || value < minimum || value > maximum) {
    throw new RangeError(`${key} must be a finite number >= ${minimum}`);
  }
  const updated = { ...inputs, [key]: value };
  if (key !== "downPaymentPercent") return updated;

  const loanPrincipal = buildPaymentSchedule(updated, config)
    .reduce((sum, payment) => sum + payment.loanAmount, 0);
  return {
    ...updated,
    monthlyEmiThousands: rupeesToRoundedThousands(
      monthlyPayment(loanPrincipal, updated.loanRate, config.defaults.loanTenureYears),
      config,
    ),
  };
}

function rupeesToRoundedThousands(value: number, config: PlanModelConfig): number {
  const step = config.simulation.monthlyAmountStepThousands;
  return Math.ceil(value / (step * 1_000)) * step;
}

function rupeesToNearestThousands(value: number, config: PlanModelConfig): number {
  const step = config.simulation.monthlyAmountStepThousands;
  return Math.max(
    step,
    Math.round(value / (step * 1_000)) * step,
  );
}

/** A plan needs a real price; without one there is nothing to finance. */
export function hasPlannablePrice(propertyPriceInr: number): boolean {
  return Number.isFinite(propertyPriceInr) && propertyPriceInr > 0;
}

/**
 * The opening plan derives rent, SIP, and EMI from one model configuration.
 * A flat EMI would either overstate monthly cost or close the loan too early.
 */
export function buildBaselinePlanInputs(
  propertyPriceInr: number,
  construction?: ConstructionProfile,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): PlanInputs {
  validatePlanModelConfig(config);
  if (!hasPlannablePrice(propertyPriceInr)) {
    throw new RangeError("propertyPriceInr must be a finite number > 0");
  }
  const defaults = config.defaults;
  const propertyPriceLakh = propertyPriceInr / LAKH;
  const estimatedRentThousands = rupeesToNearestThousands(
    propertyPriceInr * (defaults.rentalYieldPercent / 100) / MONTHS_IN_YEAR,
    config,
  );
  const monthlyEmiThousands = rupeesToRoundedThousands(
    monthlyPayment(
      propertyPriceInr * (1 - defaults.downPaymentPercent / 100),
      defaults.loanRate,
      defaults.loanTenureYears,
    ),
    config,
  );
  // Rent vs Buy starts with the simplest explicit SIP choice: 1× the EMI.
  // Buyers can compare 1×, 2× or 3× without an unbounded investment input.
  const monthlySipThousands = monthlyEmiThousands;
  return {
    propertyPriceLakh,
    downPaymentPercent: defaults.downPaymentPercent,
    monthlyEmiThousands,
    loanRate: defaults.loanRate,
    currentRentThousands: estimatedRentThousands,
    equityReturn: defaults.equityReturn,
    monthlySipThousands,
    holdingPeriodYears: defaults.holdingPeriodYears,
    purchaseYear: defaults.purchaseYear,
    construction: construction ?? {
      state: "ready",
      asOfDate: new Date().toISOString().slice(0, 10),
      dateSource: "not_applicable",
    },
    assumptions: {
      homeAppreciationRate: defaults.homeAppreciationRate,
      rentInflationRate: defaults.rentInflationRate,
    },
  };
}

export function calculateLoanJourney(
  inputs: PlanInputs,
  extraEmisPerYear?: number,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
  strategy: RepaymentStrategy = "finish_earlier",
): LoanJourney {
  inputs = normalizePlanInputs(inputs);
  extraEmisPerYear = normalizeExtraEmisPerYear(
    extraEmisPerYear ?? config.defaults.extraEmisPerYear,
  );
  const schedule = buildLoanSchedule(inputs, { extraEmisPerYear, strategy }, config);
  const baseline = buildLoanSchedule(inputs, { extraEmisPerYear: 0 }, config);
  const points: LoanJourneyPoint[] = [];
  let yearlyInterest = 0;
  let yearlyPrincipal = 0;
  let yearlyExtra = 0;
  points.push({
    year: 0,
    balance: schedule.months[0]?.openingBalance ?? 0,
    interestPaid: 0,
    principalPaid: 0,
    extraPaid: 0,
  });

  for (const month of schedule.months) {
    yearlyInterest += month.interestPaid;
    yearlyPrincipal += month.principalPaid;
    yearlyExtra += month.extraPaid;
    if (
      (month.month + 1) % MONTHS_IN_YEAR === 0
      || (
        month.paymentNumber > 0
        && month.closingBalance <= config.simulation.closedBalanceRupees
      )
    ) {
      const nextOpeningBalance = schedule.months[month.month + 1]?.openingBalance;
      points.push({
        year: Math.ceil((month.month + 1) / MONTHS_IN_YEAR),
        balance: nextOpeningBalance ?? month.closingBalance,
        interestPaid: yearlyInterest,
        principalPaid: yearlyPrincipal,
        extraPaid: yearlyExtra,
      });
      yearlyInterest = 0;
      yearlyPrincipal = 0;
      yearlyExtra = 0;
    }
  }

  const lastPlanYear = Math.min(
    config.simulation.maximumJourneyYears,
    Math.ceil((baseline.baselinePayoffMonth ?? schedule.months.length) / MONTHS_IN_YEAR),
  );
  for (let year = points.at(-1)?.year ?? 0; year < lastPlanYear; year += 1) {
    points.push({ year: year + 1, balance: 0, interestPaid: 0, principalPaid: 0, extraPaid: 0 });
  }

  const closed = schedule.payoffMonth != null;

  return {
    monthlyEmi: schedule.openingMonthlyEmi,
    endingMonthlyEmi: schedule.endingMonthlyEmi,
    annualPrepayment: schedule.annualPrepayment,
    loanFreeMonths: schedule.payoffMonth ?? schedule.months.length,
    monthsSaved: closed && baseline.payoffMonth != null
      ? Math.max(0, baseline.payoffMonth - schedule.payoffMonth!)
      : 0,
    interestSaved: closed && baseline.totalInterest != null && schedule.totalInterest != null
      ? Math.max(0, baseline.totalInterest - schedule.totalInterest)
      : null,
    totalInterest: schedule.totalInterest,
    repaymentStrategy: strategy,
    points,
  };
}

/**
 * Break-even is the year buying overtakes renting. If buying never trails there
 * is nothing to break even from, so the answer is null rather than year one.
 */
function findBreakEvenYear(points: ProjectionPoint[], purchaseYear: number): number | null {
  let rentHasLed = false;
  for (const point of points) {
    if (point.year <= purchaseYear) continue;
    if (point.buyNetWorth < point.rentNetWorth) {
      rentHasLed = true;
    } else if (rentHasLed) {
      return point.year;
    }
  }
  return null;
}

export function calculateProjection(
  inputs: PlanInputs,
  extraEmisPerYear?: number,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
  strategy: RepaymentStrategy = "finish_earlier",
): PlanProjection {
  inputs = normalizePlanInputs(inputs);
  extraEmisPerYear = normalizeExtraEmisPerYear(
    extraEmisPerYear ?? config.defaults.extraEmisPerYear,
  );
  const schedule = buildPaymentSchedule(inputs, config);
  const loanAmount = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const upfrontPayment = schedule.reduce((sum, payment) => sum + payment.cashAmount, 0);
  const journey = calculateLoanJourney(inputs, extraEmisPerYear, config, strategy);
  const totalInterest = journey.totalInterest;
  const points = calculateProjectionPoints(inputs, extraEmisPerYear, config, strategy);
  const constructionPlan = constructionPlanFor(inputs, config);
  const loanFreeYear = journey.totalInterest == null
    ? null
    : Math.ceil(journey.loanFreeMonths / MONTHS_IN_YEAR);

  return {
    monthlyEmi,
    monthlyRent: inputs.currentRentThousands * 1_000,
    monthlySip: inputs.monthlySipThousands * 1_000,
    loanAmount,
    upfrontPayment,
    totalInterest,
    breakEvenYear: findBreakEvenYear(points, inputs.purchaseYear),
    loanFreeYear,
    extraEmisPerYear,
    repaymentStrategy: strategy,
    endingMonthlyEmi: journey.endingMonthlyEmi,
    possessionMonth: constructionPlan.possessionMonth,
    possessionDate: constructionPlan.possessionDate,
    constructionDateSource: constructionPlan.dateSource,
    assumptions: inputs.assumptions,
    paymentSchedule: schedule,
    points,
  };
}

export function formatCurrency(value: number, compact = false): string {
  if (compact && Math.abs(value) >= 10_000_000) {
    return `₹${(value / 10_000_000).toFixed(2)}Cr`;
  }
  if (compact && Math.abs(value) >= LAKH) {
    return `₹${(value / LAKH).toFixed(1)}L`;
  }
  return new Intl.NumberFormat("en-IN", {
    style: "currency",
    currency: "INR",
    maximumFractionDigits: 0,
  }).format(value);
}
