import {
  buildPaymentSchedule,
  calculateFinancingInterest,
  calculateProjectionPoints,
  constructionPlanFor,
  monthsToPayoff,
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
  monthlyRent: number;
  monthlySip: number;
  loanAmount: number;
  upfrontPayment: number;
  totalInterest: number;
  breakEvenYear: number | null;
  /** First year the loan is cleared within the horizon, if ever. */
  loanFreeYear: number | null;
  possessionMonth: number;
  possessionDate: string | null;
  constructionDateSource: ConstructionProfile["dateSource"];
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
  interestSaved: number;
  totalInterest: number;
  points: LoanJourneyPoint[];
};

const MONTHS_IN_YEAR = 12;
const LAKH = 100_000;

export function buildBaselinePlanInputs(
  propertyPriceInr: number,
  construction?: ConstructionProfile,
): PlanInputs {
  const propertyPriceLakh = Math.max(20, propertyPriceInr / LAKH);
  const estimatedRentThousands = Math.max(
    20,
    Math.round((propertyPriceInr * 0.032 / MONTHS_IN_YEAR) / 1_000 / 5) * 5,
  );
  return {
    propertyPriceLakh,
    monthlyEmiThousands: 90,
    loanRate: 7.5,
    currentRentThousands: estimatedRentThousands,
    equityReturn: 10,
    monthlySipThousands: 90,
    holdingPeriodYears: 20,
    purchaseYear: 0,
    construction: construction ?? {
      state: "ready",
      asOfDate: new Date().toISOString().slice(0, 10),
      dateSource: "not_applicable",
    },
  };
}

export function calculateLoanJourney(
  inputs: PlanInputs,
  extraEmisPerYear: number,
): LoanJourney {
  const schedule = buildPaymentSchedule(inputs);
  const constructionPlan = constructionPlanFor(inputs);
  const principal = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const repaymentMonths = monthsToPayoff(principal, inputs.loanRate, monthlyEmi);
  const maxSimMonths = constructionPlan.possessionMonth + 40 * MONTHS_IN_YEAR;
  const baselineLoanFreeMonth = Number.isFinite(repaymentMonths)
    ? constructionPlan.possessionMonth + repaymentMonths
    : maxSimMonths;
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
    && month < maxSimMonths
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

  const lastPlanYear = Math.min(
    40,
    Math.ceil(baselineLoanFreeMonth / MONTHS_IN_YEAR),
  );
  for (let year = points.at(-1)?.year ?? 0; year < lastPlanYear; year += 1) {
    points.push({ year: year + 1, balance: 0, interestPaid: 0, principalPaid: 0, extraPaid: 0 });
  }

  const originalInterest = calculateFinancingInterest(inputs);

  return {
    monthlyEmi,
    annualPrepayment,
    loanFreeMonths: month,
    monthsSaved: Number.isFinite(repaymentMonths)
      ? Math.max(0, baselineLoanFreeMonth - month)
      : 0,
    interestSaved: Math.max(0, originalInterest - totalInterest),
    totalInterest,
    points,
  };
}

export function calculateProjection(
  inputs: PlanInputs,
  extraEmisPerYear = 0,
): PlanProjection {
  const schedule = buildPaymentSchedule(inputs);
  const loanAmount = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const monthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const upfrontPayment = schedule.reduce((sum, payment) => sum + payment.cashAmount, 0);
  const totalInterest = calculateFinancingInterest(inputs);
  const points = calculateProjectionPoints(inputs, extraEmisPerYear);
  const breakEvenPoint = points.find((point) => (
    point.year > inputs.purchaseYear && point.buyNetWorth >= point.rentNetWorth
  ));
  const loanFreePoint = points.find((point) => (
    point.year > 0 && point.loanBalance <= 0.5
  ));
  const constructionPlan = constructionPlanFor(inputs);

  return {
    monthlyEmi,
    monthlyRent: inputs.currentRentThousands * 1_000,
    monthlySip: inputs.monthlySipThousands * 1_000,
    loanAmount,
    upfrontPayment,
    totalInterest,
    breakEvenYear: breakEvenPoint?.year ?? null,
    loanFreeYear: loanFreePoint?.year ?? null,
    possessionMonth: constructionPlan.possessionMonth,
    possessionDate: constructionPlan.possessionDate,
    constructionDateSource: constructionPlan.dateSource,
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
  monthlySipThousands: 90,
  holdingPeriodYears: 15,
  purchaseYear: 0,
  construction: {
    state: "ready",
    asOfDate: "2026-01-01",
    dateSource: "not_applicable",
  },
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
