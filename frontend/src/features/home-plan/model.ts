import {
  buildPaymentSchedule,
  calculateFinancingInterest,
  calculateProjectionPoints,
  constructionPlanFor,
} from "./financeEngine.ts";

export type ConstructionProfile = {
  state: "ready" | "under_construction";
  asOfDate: string;
  startDate?: string;
  completionDate?: string;
  dateSource: "rera" | "estimated" | "not_applicable";
};

export type PlanInputs = {
  propertyPriceLakh: number;
  startingSavingsLakh: number;
  downPaymentLakh: number;
  loanRate: number;
  loanTenureYears: number;
  currentRentThousands: number;
  rentInflation: number;
  appreciation: number;
  equityReturn: number;
  monthlyExtraInvestmentThousands: number;
  holdingPeriodYears: number;
  purchaseYear: number;
  construction: ConstructionProfile;
};

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
  loanAmount: number;
  totalInterest: number;
  breakEvenYear: number | null;
  liquidityAfterDownPayment: number;
  opportunityCost: number;
  possessionMonth: number;
  possessionDate: string | null;
  constructionDateSource: ConstructionProfile["dateSource"];
  paymentSchedule: BuilderPayment[];
  points: ProjectionPoint[];
  sensitivity: SensitivityCell[];
};

export type BuilderPayment = {
  month: number;
  date: string;
  amount: number;
  cashAmount: number;
  loanAmount: number;
};

export type SensitivityCell = {
  appreciation: number;
  equityReturn: number;
  difference: number;
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
  interestSaved: number;
  totalInterest: number;
  points: LoanJourneyPoint[];
};

const MONTHS_IN_YEAR = 12;
const LAKH = 100_000;
const STARTING_LIQUID_SAVINGS_LAKH = 58;
const PURCHASE_COST_RATE = 0.07;

export function buildBaselinePlanInputs(
  propertyPriceInr: number,
  construction?: ConstructionProfile,
): PlanInputs {
  const propertyPriceLakh = Math.max(20, propertyPriceInr / LAKH);
  const minimumDownLakh = minimumDownPaymentLakh(propertyPriceLakh);
  const desiredDownLakh = Math.max(
    minimumDownLakh,
    Math.round(propertyPriceLakh * 0.27 / 5) * 5,
  );
  const purchaseCostsLakh = propertyPriceLakh * PURCHASE_COST_RATE;
  const startingSavingsLakh = Math.max(
    STARTING_LIQUID_SAVINGS_LAKH,
    Math.ceil((desiredDownLakh + purchaseCostsLakh + 5) / 5) * 5,
  );
  const estimatedRentThousands = Math.max(
    20,
    Math.round((propertyPriceInr * 0.032 / MONTHS_IN_YEAR) / 1_000 / 5) * 5,
  );
  const inputs: PlanInputs = {
    propertyPriceLakh,
    startingSavingsLakh,
    downPaymentLakh: desiredDownLakh,
    loanRate: 8.4,
    loanTenureYears: 20,
    currentRentThousands: estimatedRentThousands,
    rentInflation: 10,
    appreciation: 6.5,
    equityReturn: 10,
    monthlyExtraInvestmentThousands: 0,
    holdingPeriodYears: 20,
    purchaseYear: 0,
    construction: construction ?? {
      state: "ready",
      asOfDate: new Date().toISOString().slice(0, 10),
      dateSource: "not_applicable",
    },
  };
  inputs.downPaymentLakh = Math.min(inputs.downPaymentLakh, maximumDownPaymentLakh(inputs));
  return inputs;
}

export function calculateUpfrontCash(inputs: PlanInputs): number {
  const propertyPrice = inputs.propertyPriceLakh * LAKH;
  const purchasePrice = compound(propertyPrice, inputs.appreciation, inputs.purchaseYear);
  const firstPayment = buildPaymentSchedule(inputs).at(0)?.cashAmount ?? 0;
  return firstPayment + purchasePrice * PURCHASE_COST_RATE;
}

export function maximumDownPaymentLakh(inputs: PlanInputs): number {
  const purchaseCostsLakh = inputs.propertyPriceLakh * PURCHASE_COST_RATE;
  const availableLakh = Math.max(0, inputs.startingSavingsLakh - purchaseCostsLakh);
  return Math.max(0, Math.min(inputs.propertyPriceLakh * 0.8, Math.floor(availableLakh / 5) * 5));
}

export function minimumDownPaymentLakh(propertyPriceLakh: number): number {
  const minimumRate = propertyPriceLakh <= 30
    ? 0.1
    : propertyPriceLakh <= 75
      ? 0.2
      : 0.25;
  return Math.ceil(propertyPriceLakh * minimumRate / 5) * 5;
}

export function minimumRequiredSavingsLakh(inputs: PlanInputs): number {
  return minimumDownPaymentLakh(inputs.propertyPriceLakh)
    + inputs.propertyPriceLakh * PURCHASE_COST_RATE;
}

function compound(value: number, annualRate: number, years: number): number {
  return value * (1 + annualRate / 100) ** years;
}

function monthlyPayment(principal: number, annualRate: number, years: number): number {
  const months = years * MONTHS_IN_YEAR;
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (monthlyRate === 0) return principal / months;
  const growth = (1 + monthlyRate) ** months;
  return principal * monthlyRate * growth / (growth - 1);
}

export function calculateLoanJourney(
  inputs: PlanInputs,
  extraEmisPerYear: number,
): LoanJourney {
  const schedule = buildPaymentSchedule(inputs);
  const constructionPlan = constructionPlanFor(inputs);
  const principal = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = monthlyPayment(principal, inputs.loanRate, inputs.loanTenureYears);
  const repaymentMonths = inputs.loanTenureYears * MONTHS_IN_YEAR;
  const baselineLoanFreeMonth = constructionPlan.possessionMonth + repaymentMonths;
  const paymentsByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const annualPrepayment = monthlyEmi * extraEmisPerYear;
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
  ) {
    const interest = balance * monthlyRate;
    const hasPossession = month >= constructionPlan.possessionMonth;
    const regularPayment = hasPossession ? Math.min(monthlyEmi, balance + interest) : interest;
    const principalPayment = Math.max(0, regularPayment - interest);
    balance = Math.max(0, balance - principalPayment);
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

  const lastPlanYear = Math.ceil(baselineLoanFreeMonth / MONTHS_IN_YEAR);
  for (let year = points.at(-1)?.year ?? 0; year < lastPlanYear; year += 1) {
    points.push({ year: year + 1, balance: 0, interestPaid: 0, principalPaid: 0, extraPaid: 0 });
  }

  const originalInterest = calculateFinancingInterest(inputs);

  return {
    monthlyEmi,
    annualPrepayment,
    loanFreeMonths: month,
    monthsSaved: Math.max(0, baselineLoanFreeMonth - month),
    interestSaved: Math.max(0, originalInterest - totalInterest),
    totalInterest,
    points,
  };
}

function calculatePoints(inputs: PlanInputs): ProjectionPoint[] {
  return calculateProjectionPoints(inputs);
}

function terminalDifference(inputs: PlanInputs): number {
  const points = calculatePoints(inputs);
  const terminal = points.at(-1);
  return terminal ? terminal.buyNetWorth - terminal.rentNetWorth : 0;
}

export function calculateProjection(inputs: PlanInputs): PlanProjection {
  const propertyPrice = inputs.propertyPriceLakh * LAKH;
  const downPayment = inputs.downPaymentLakh * LAKH;
  const purchasePrice = compound(propertyPrice, inputs.appreciation, inputs.purchaseYear);
  const loanAmount = Math.max(0, purchasePrice - downPayment);
  const monthlyEmi = monthlyPayment(loanAmount, inputs.loanRate, inputs.loanTenureYears);
  const totalInterest = calculateFinancingInterest(inputs);
  const points = calculatePoints(inputs);
  const breakEvenPoint = points.find((point) => point.year > inputs.purchaseYear && point.buyNetWorth >= point.rentNetWorth);
  const investedDownPayment = compound(downPayment, inputs.equityReturn, inputs.holdingPeriodYears);
  const propertyDownPaymentValue = compound(downPayment, inputs.appreciation, inputs.holdingPeriodYears);

  const sensitivity = [4, 6, 8].flatMap((appreciation) =>
    [8, 10, 12].map((equityReturn) => ({
      appreciation,
      equityReturn,
      difference: terminalDifference({ ...inputs, appreciation, equityReturn }),
    })),
  );

  const savingsAtPurchase = compound(
    inputs.startingSavingsLakh * LAKH,
    inputs.equityReturn,
    inputs.purchaseYear,
  );
  const constructionPlan = constructionPlanFor(inputs);
  const paymentSchedule = buildPaymentSchedule(inputs);

  return {
    monthlyEmi,
    loanAmount,
    totalInterest,
    breakEvenYear: breakEvenPoint?.year ?? null,
    liquidityAfterDownPayment: Math.max(
      0,
      savingsAtPurchase - downPayment - purchasePrice * PURCHASE_COST_RATE,
    ),
    opportunityCost: investedDownPayment - propertyDownPaymentValue,
    possessionMonth: constructionPlan.possessionMonth,
    possessionDate: constructionPlan.possessionDate,
    constructionDateSource: constructionPlan.dateSource,
    paymentSchedule,
    points,
    sensitivity,
  };
}

export function calculateScenarioGap(inputs: PlanInputs, year: number): number {
  const points = calculatePoints(inputs);
  const point = points[Math.min(year, points.length - 1)];
  return point.buyNetWorth - point.rentNetWorth;
}

export const BASE_INPUTS: PlanInputs = {
  propertyPriceLakh: 150,
  startingSavingsLakh: 58,
  downPaymentLakh: 40,
  loanRate: 8.4,
  loanTenureYears: 20,
  currentRentThousands: 55,
  rentInflation: 10,
  appreciation: 6.5,
  equityReturn: 10,
  monthlyExtraInvestmentThousands: 0,
  holdingPeriodYears: 15,
  purchaseYear: 0,
  construction: {
    state: "ready",
    asOfDate: "2026-01-01",
    dateSource: "not_applicable",
  },
};

export const SCENARIO_PRESETS: Record<string, PlanInputs> = {
  Base: BASE_INPUTS,
  Conservative: { ...BASE_INPUTS, appreciation: 4.5, equityReturn: 9, loanRate: 9.1 },
  Optimistic: { ...BASE_INPUTS, appreciation: 8, equityReturn: 11, loanRate: 7.8 },
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
