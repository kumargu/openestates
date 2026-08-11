import {
  buildPaymentSchedule,
  calculateFinancingInterest,
  calculateProjectionPoints,
  constructionPlanFor,
  DEFAULT_HOME_APPRECIATION_RATE,
  DEFAULT_LOAN_TENURE_YEARS,
  DEFAULT_RENT_INFLATION_RATE,
  monthlyPayment,
  monthsToPayoff,
} from "./financeEngine.ts";

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
  annualPrepayment: number;
  loanFreeMonths: number;
  monthsSaved: number;
  interestSaved: number | null;
  totalInterest: number | null;
  points: LoanJourneyPoint[];
};

const MONTHS_IN_YEAR = 12;
const LAKH = 100_000;

const DEFAULT_MONTHLY_EMI_THOUSANDS = 90;
const MIN_DEFAULT_MONTHLY_SIP_THOUSANDS = 35;
const DEFAULT_LOAN_RATE = 7.5;

export const DEFAULT_PLAN_ASSUMPTIONS: PlanAssumptions = {
  homeAppreciationRate: DEFAULT_HOME_APPRECIATION_RATE,
  rentInflationRate: DEFAULT_RENT_INFLATION_RATE,
};

function finiteAmount(value: number, field: string, min = 0): number {
  if (!Number.isFinite(value) || value < min) {
    throw new RangeError(`${field} must be a finite number >= ${min}`);
  }
  return value;
}

function normalizeExtraEmisPerYear(value: number): number {
  if (!Number.isFinite(value) || value < 0 || !Number.isInteger(value)) {
    throw new RangeError("extraEmisPerYear must be a finite whole number >= 0");
  }
  return value;
}

export function normalizePlanInputs(inputs: PlanInputs): PlanInputs {
  const propertyPriceLakh = finiteAmount(inputs.propertyPriceLakh, "propertyPriceLakh", 0.01);
  return {
    ...inputs,
    propertyPriceLakh,
    monthlyEmiThousands: finiteAmount(inputs.monthlyEmiThousands, "monthlyEmiThousands", 1),
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

/** Updates exactly the input the buyer changed. */
export function updatePlanInput(
  inputs: PlanInputs,
  key: EditablePlanInput,
  value: number,
): PlanInputs {
  const minimum = key === "monthlyEmiThousands" ? 1 : 0;
  if (!Number.isFinite(value) || value < minimum) {
    throw new RangeError(`${key} must be a finite number >= ${minimum}`);
  }
  return { ...inputs, [key]: value };
}

function rupeesToRoundedThousands(value: number): number {
  return Math.ceil(value / 5_000) * 5;
}

export function buildBaselinePlanInputs(
  propertyPriceInr: number,
  construction?: ConstructionProfile,
): PlanInputs {
  const propertyPriceLakh = Math.max(20, propertyPriceInr / LAKH);
  const estimatedRentThousands = Math.max(
    20,
    Math.round((propertyPriceInr * 0.032 / MONTHS_IN_YEAR) / 1_000 / 5) * 5,
  );
  const amortizingEmiThousands = rupeesToRoundedThousands(
    monthlyPayment(propertyPriceInr, DEFAULT_LOAN_RATE, DEFAULT_LOAN_TENURE_YEARS),
  );
  // Keep the rent-path cash out aligned with the buy EMI by default, while
  // preserving a visible SIP even when the estimated rent is high.
  const monthlyEmiThousands = Math.max(
    DEFAULT_MONTHLY_EMI_THOUSANDS,
    amortizingEmiThousands,
    estimatedRentThousands + MIN_DEFAULT_MONTHLY_SIP_THOUSANDS,
  );
  const monthlySipThousands = Math.max(
    0,
    monthlyEmiThousands - estimatedRentThousands,
  );
  return {
    propertyPriceLakh,
    monthlyEmiThousands,
    loanRate: DEFAULT_LOAN_RATE,
    currentRentThousands: estimatedRentThousands,
    equityReturn: 10,
    monthlySipThousands,
    holdingPeriodYears: 20,
    purchaseYear: 0,
    construction: construction ?? {
      state: "ready",
      asOfDate: new Date().toISOString().slice(0, 10),
      dateSource: "not_applicable",
    },
    assumptions: { ...DEFAULT_PLAN_ASSUMPTIONS },
  };
}

export function calculateLoanJourney(
  inputs: PlanInputs,
  extraEmisPerYear: number,
): LoanJourney {
  inputs = normalizePlanInputs(inputs);
  extraEmisPerYear = normalizeExtraEmisPerYear(extraEmisPerYear);
  const schedule = buildPaymentSchedule(inputs);
  const constructionPlan = constructionPlanFor(inputs);
  const principal = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const repaymentMonths = monthsToPayoff(principal, inputs.loanRate, monthlyEmi);
  const maxSimMonths = constructionPlan.possessionMonth + 60 * MONTHS_IN_YEAR;
  const baselineLoanFreeMonth = Number.isFinite(repaymentMonths)
    ? constructionPlan.possessionMonth + repaymentMonths
    : maxSimMonths;
  const paymentsByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const annualPrepayment = monthlyEmi * Math.max(0, extraEmisPerYear);
  const points: LoanJourneyPoint[] = [];
  let balance = paymentsByMonth.get(0)?.loanAmount ?? 0;
  let month = 0;
  let totalInterest = 0;
  let yearlyInterest = 0;
  let yearlyPrincipal = 0;
  let yearlyExtra = 0;
  points.push({ year: 0, balance, interestPaid: 0, principalPaid: 0, extraPaid: 0 });

  while (
    (month < constructionPlan.possessionMonth || balance > 0.5)
    && month < baselineLoanFreeMonth
    && month < maxSimMonths
  ) {
    const interest = balance * monthlyRate;
    const hasPossession = month >= constructionPlan.possessionMonth;
    const regularPayment = hasPossession ? Math.min(monthlyEmi, balance + interest) : interest;
    const principalPayment = Math.max(0, regularPayment - interest);
    balance = Math.max(0, balance + interest - regularPayment);
    totalInterest += interest;
    yearlyInterest += interest;
    yearlyPrincipal += principalPayment;

    const paymentNumber = hasPossession ? month - constructionPlan.possessionMonth + 1 : 0;
    if (
      paymentNumber > 0
      && paymentNumber % MONTHS_IN_YEAR === 0
      && balance > 0
      && extraEmisPerYear > 0
    ) {
      yearlyExtra = Math.min(balance, annualPrepayment);
      balance -= yearlyExtra;
    }

    month += 1;
    balance += paymentsByMonth.get(month)?.loanAmount ?? 0;

    if (
      month % MONTHS_IN_YEAR === 0
      || (month >= constructionPlan.possessionMonth && balance <= 0.5)
    ) {
      points.push({
        year: Math.ceil(month / MONTHS_IN_YEAR),
        balance,
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
    40,
    Math.ceil(baselineLoanFreeMonth / MONTHS_IN_YEAR),
  );
  for (let year = points.at(-1)?.year ?? 0; year < lastPlanYear; year += 1) {
    points.push({ year: year + 1, balance: 0, interestPaid: 0, principalPaid: 0, extraPaid: 0 });
  }

  const originalInterest = calculateFinancingInterest(inputs, 0);
  const closed = balance <= 0.5;

  return {
    monthlyEmi,
    annualPrepayment,
    loanFreeMonths: month,
    monthsSaved: closed && Number.isFinite(repaymentMonths)
      ? Math.max(0, baselineLoanFreeMonth - month)
      : 0,
    interestSaved: closed && originalInterest != null
      ? Math.max(0, originalInterest - totalInterest)
      : null,
    totalInterest: closed ? totalInterest : null,
    points,
  };
}

export function calculateProjection(
  inputs: PlanInputs,
  extraEmisPerYear = 0,
): PlanProjection {
  inputs = normalizePlanInputs(inputs);
  extraEmisPerYear = normalizeExtraEmisPerYear(extraEmisPerYear);
  const schedule = buildPaymentSchedule(inputs);
  const loanAmount = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const upfrontPayment = schedule.reduce((sum, payment) => sum + payment.cashAmount, 0);
  const journey = calculateLoanJourney(inputs, extraEmisPerYear);
  const totalInterest = journey.totalInterest;
  const points = calculateProjectionPoints(inputs, extraEmisPerYear);
  const breakEvenPoint = points.find((point) => (
    point.year > inputs.purchaseYear && point.buyNetWorth >= point.rentNetWorth
  ));
  const constructionPlan = constructionPlanFor(inputs);
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
    breakEvenYear: breakEvenPoint?.year ?? null,
    loanFreeYear,
    extraEmisPerYear,
    possessionMonth: constructionPlan.possessionMonth,
    possessionDate: constructionPlan.possessionDate,
    constructionDateSource: constructionPlan.dateSource,
    assumptions: inputs.assumptions,
    paymentSchedule: schedule,
    points,
  };
}

export const BASE_INPUTS: PlanInputs = {
  propertyPriceLakh: 150,
  monthlyEmiThousands: 90,
  loanRate: 7.5,
  currentRentThousands: 55,
  equityReturn: 10,
  monthlySipThousands: 35,
  holdingPeriodYears: 15,
  purchaseYear: 0,
  construction: {
    state: "ready",
    asOfDate: "2026-01-01",
    dateSource: "not_applicable",
  },
  assumptions: { ...DEFAULT_PLAN_ASSUMPTIONS },
};

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
