import type {
  BuilderPayment,
  ConstructionProfile,
  PlanInputs,
  ProjectionPoint,
} from "./model.ts";

/**
 * Rent vs buy algorithm (monthly)
 *
 * User levers:
 * - down payment percentage
 * - monthly EMI
 * - monthly rent (cash-out context; under-construction housing cost)
 * - monthly SIP on the rent path
 * - loan rate
 * - SIP return
 * - extra EMIs per year
 *
 * Financing story (deliberately simple):
 * - The down payment is paid in cash at each builder milestone.
 * - EMI + rate decide how fast the remaining loan clears.
 * - Extra EMIs each year pull the loan-free date forward.
 *
 * Wealth, on one rule: each path starts with the same cash and commits the
 * same money every month, and whatever housing does not consume is invested
 * at the SIP return.
 * - The buyer commits the EMI. Once the loan closes, that EMI is invested.
 * - The renter commits rent + SIP. As rent rises, less of it is left to invest.
 * - Buy = home value − loan left + what the buyer invested
 * - Rent = the matching down-payment cash + what the renter invested
 *
 * Without that rule the comparison drifts: a renter whose rent has tripled would
 * still be credited with the original SIP, and a buyer with a closed loan would
 * be credited with nothing, so renting would win on spending more money.
 *
 * Under-construction homes still use a 6-month builder schedule.
 * Until possession the buyer pays rent + pre-EMI interest instead of EMI.
 */

const LAKH = 100_000;
const MONTHS_IN_YEAR = 12;
const PAYMENT_INTERVAL_MONTHS = 6;
const DEFAULT_REMAINING_CONSTRUCTION_MONTHS = 24;
const DEFAULT_TOTAL_CONSTRUCTION_MONTHS = 36;
const MINIMUM_BOOKING_RATE = 0.1;
const MAX_LOAN_SIMULATION_YEARS = 60;
export const DEFAULT_LOAN_TENURE_YEARS = 20;
export const DEFAULT_HOME_APPRECIATION_RATE = 6;
export const DEFAULT_RENT_INFLATION_RATE = 10;
export const FIXED_HOME_GROWTH_RATE = DEFAULT_HOME_APPRECIATION_RATE;
export const FIXED_RENT_INFLATION_RATE = DEFAULT_RENT_INFLATION_RATE;

type ConstructionPlan = {
  startDate: Date;
  purchaseDate: Date;
  possessionDateValue: Date;
  possessionDate: string | null;
  purchaseMonth: number;
  possessionMonth: number;
  dateSource: ConstructionProfile["dateSource"];
};

export function isExplicitlyReadyStatus(value: string): boolean {
  const normalized = value.toLowerCase().replace(/[_-]+/g, " ").replace(/\s+/g, " ").trim();
  if (/\b(not|isn't|is not|incomplete|under)\b.*\b(complete|completed|delivered|ready)\b/.test(normalized)) {
    return false;
  }
  return /^(ready|ready to move|delivered|completed|complete|completed project|project completed)(\s*[·|—-].*)?$/
    .test(normalized);
}

export function parsePlanDate(value?: string): Date | null {
  if (!value) return null;
  const normalized = value.trim();
  const isoMatch = normalized.match(/^(\d{4})-(\d{1,2})-(\d{1,2})/);
  const indianMatch = normalized.match(/^(\d{1,2})[/-](\d{1,2})[/-](\d{4})$/);
  const parts = isoMatch
    ? [Number(isoMatch[1]), Number(isoMatch[2]), Number(isoMatch[3])]
    : indianMatch
      ? [Number(indianMatch[3]), Number(indianMatch[2]), Number(indianMatch[1])]
      : null;
  if (!parts) return null;
  const [year, month, day] = parts;
  const parsed = new Date(Date.UTC(year, month - 1, day));
  return (
    parsed.getUTCFullYear() === year
    && parsed.getUTCMonth() === month - 1
    && parsed.getUTCDate() === day
  ) ? parsed : null;
}

function isoDate(value: Date): string {
  return value.toISOString().slice(0, 10);
}

function addMonths(value: Date, months: number): Date {
  const result = new Date(value);
  result.setUTCMonth(result.getUTCMonth() + months);
  return result;
}

function monthsBetween(from: Date, to: Date): number {
  const roughMonths = (
    (to.getUTCFullYear() - from.getUTCFullYear()) * MONTHS_IN_YEAR
    + to.getUTCMonth()
    - from.getUTCMonth()
  );
  const dayAdjustment = (to.getUTCDate() - from.getUTCDate()) / 31;
  return Math.max(0, Math.ceil(roughMonths + dayAdjustment));
}

function compoundMonthly(value: number, annualRate: number, months: number): number {
  return value * (1 + annualRate / 100 / MONTHS_IN_YEAR) ** months;
}

/** Rent rises once a year (Bangalore-style), not every month. */
export function rentInMonth(startingMonthlyRent: number, rentInflation: number, month: number): number {
  const yearsElapsed = Math.floor(Math.max(0, month) / MONTHS_IN_YEAR);
  return Math.round(startingMonthlyRent * (1 + rentInflation / 100) ** yearsElapsed);
}

export function monthlyPayment(principal: number, annualRate: number, years: number): number {
  if (principal <= 0) return 0;
  const months = years * MONTHS_IN_YEAR;
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (monthlyRate === 0) return principal / months;
  const growth = (1 + monthlyRate) ** months;
  return principal * monthlyRate * growth / (growth - 1);
}

export function principalFromMonthlyPayment(
  payment: number,
  annualRate: number,
  years: number,
): number {
  if (payment <= 0) return 0;
  const months = years * MONTHS_IN_YEAR;
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (monthlyRate === 0) return payment * months;
  return payment * (1 - (1 + monthlyRate) ** -months) / monthlyRate;
}

export function monthsToPayoff(
  principal: number,
  annualRate: number,
  monthlyPaymentAmount: number,
): number {
  if (principal <= 0) return 0;
  if (monthlyPaymentAmount <= 0) return Number.POSITIVE_INFINITY;
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (monthlyRate === 0) return Math.ceil(principal / monthlyPaymentAmount);
  if (monthlyPaymentAmount <= principal * monthlyRate) return Number.POSITIVE_INFINITY;
  return Math.ceil(
    -Math.log(1 - principal * monthlyRate / monthlyPaymentAmount)
    / Math.log(1 + monthlyRate),
  );
}

export function constructionPlanFor(inputs: PlanInputs): ConstructionPlan {
  const asOfDate = parsePlanDate(inputs.construction.asOfDate) ?? new Date("2026-01-01T00:00:00Z");
  const purchaseMonth = Math.max(0, Math.round(inputs.purchaseYear * MONTHS_IN_YEAR));
  const purchaseDate = addMonths(asOfDate, purchaseMonth);

  if (inputs.construction.state === "ready") {
    return {
      startDate: purchaseDate,
      purchaseDate,
      possessionDateValue: purchaseDate,
      possessionDate: isoDate(purchaseDate),
      purchaseMonth,
      possessionMonth: purchaseMonth,
      dateSource: "not_applicable",
    };
  }

  const suppliedCompletion = parsePlanDate(inputs.construction.completionDate);
  if (suppliedCompletion && suppliedCompletion <= purchaseDate) {
    return {
      startDate: suppliedCompletion,
      purchaseDate,
      possessionDateValue: purchaseDate,
      possessionDate: isoDate(purchaseDate),
      purchaseMonth,
      possessionMonth: purchaseMonth,
      dateSource: inputs.construction.dateSource,
    };
  }
  const possessionDateValue = suppliedCompletion && suppliedCompletion > purchaseDate
    ? suppliedCompletion
    : addMonths(purchaseDate, DEFAULT_REMAINING_CONSTRUCTION_MONTHS);
  const suppliedStart = parsePlanDate(inputs.construction.startDate);
  const startDate = suppliedStart && suppliedStart < possessionDateValue
    ? suppliedStart
    : addMonths(possessionDateValue, -DEFAULT_TOTAL_CONSTRUCTION_MONTHS);
  const usedEstimate = !suppliedCompletion || suppliedCompletion <= purchaseDate || !suppliedStart;

  return {
    startDate,
    purchaseDate,
    possessionDateValue,
    possessionDate: isoDate(possessionDateValue),
    purchaseMonth,
    possessionMonth: purchaseMonth + monthsBetween(purchaseDate, possessionDateValue),
    dateSource: usedEstimate ? "estimated" : inputs.construction.dateSource,
  };
}

export function buildPaymentSchedule(inputs: PlanInputs): BuilderPayment[] {
  const plan = constructionPlanFor(inputs);
  const purchasePrice = compoundMonthly(
    inputs.propertyPriceLakh * LAKH,
    inputs.assumptions.homeAppreciationRate,
    plan.purchaseMonth,
  );
  const downPaymentRate = inputs.downPaymentPercent / 100;
  const requestedCash = purchasePrice * downPaymentRate;
  const requestedLoan = purchasePrice - requestedCash;

  if (plan.possessionMonth === plan.purchaseMonth) {
    return [{
      month: plan.purchaseMonth,
      date: isoDate(plan.purchaseDate),
      amount: purchasePrice,
      cashAmount: requestedCash,
      loanAmount: requestedLoan,
    }];
  }

  const totalDuration = Math.max(1, plan.possessionDateValue.getTime() - plan.startDate.getTime());
  const elapsedAtPurchase = Math.max(
    0,
    (plan.purchaseDate.getTime() - plan.startDate.getTime()) / totalDuration,
  );
  const eventMonths = [plan.purchaseMonth];
  for (
    let month = plan.purchaseMonth + PAYMENT_INTERVAL_MONTHS;
    month < plan.possessionMonth;
    month += PAYMENT_INTERVAL_MONTHS
  ) {
    eventMonths.push(month);
  }
  eventMonths.push(plan.possessionMonth);

  let previousCumulativeRate = 0;
  let cashRemaining = requestedCash;
  let loanRemaining = requestedLoan;
  return eventMonths.map((month, index) => {
    const eventDate = month === plan.possessionMonth
      ? plan.possessionDateValue
      : addMonths(plan.purchaseDate, month - plan.purchaseMonth);
    const elapsedRate = (
      (eventDate.getTime() - plan.startDate.getTime()) / totalDuration
    );
    const cumulativeRate = index === 0
      ? Math.min(1, Math.max(MINIMUM_BOOKING_RATE, elapsedAtPurchase))
      : Math.min(1, Math.max(previousCumulativeRate, elapsedRate));
    const amount = index === eventMonths.length - 1
      ? purchasePrice * (1 - previousCumulativeRate)
      : purchasePrice * (cumulativeRate - previousCumulativeRate);
    previousCumulativeRate = cumulativeRate;
    const cashAmount = index === eventMonths.length - 1
      ? cashRemaining
      : Math.min(cashRemaining, amount * downPaymentRate);
    cashRemaining = Math.max(0, cashRemaining - cashAmount);
    const loanAmount = index === eventMonths.length - 1
      ? loanRemaining
      : Math.min(loanRemaining, amount - cashAmount);
    loanRemaining = Math.max(0, loanRemaining - loanAmount);

    return {
      month,
      date: isoDate(eventDate),
      amount,
      cashAmount,
      loanAmount,
    };
  });
}

export function calculateFinancingInterest(
  inputs: PlanInputs,
  extraEmisPerYear = 0,
): number | null {
  const plan = constructionPlanFor(inputs);
  const schedule = buildPaymentSchedule(inputs);
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const paymentsByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  const emi = inputs.monthlyEmiThousands * 1_000;
  const annualPrepayment = emi * Math.max(0, extraEmisPerYear);
  const maxMonth = plan.possessionMonth + MAX_LOAN_SIMULATION_YEARS * MONTHS_IN_YEAR;
  let balance = 0;
  let totalInterest = 0;

  for (let month = plan.purchaseMonth; month <= maxMonth; month += 1) {
    balance += paymentsByMonth.get(month)?.loanAmount ?? 0;
    if (month >= plan.possessionMonth && balance <= 0.5) return totalInterest;

    const interest = balance * monthlyRate;
    if (month < plan.possessionMonth) {
      totalInterest += interest;
      continue;
    }

    if (emi <= interest && annualPrepayment <= 0) return null;
    const payment = Math.min(emi, balance + interest);
    totalInterest += interest;
    balance = Math.max(0, balance + interest - payment);

    const paymentNumber = month - plan.possessionMonth + 1;
    if (
      paymentNumber > 0
      && paymentNumber % MONTHS_IN_YEAR === 0
      && balance > 0.5
      && annualPrepayment > 0
    ) {
      balance = Math.max(0, balance - Math.min(balance, annualPrepayment));
    }
  }

  return null;
}

export function calculateProjectionPoints(
  inputs: PlanInputs,
  extraEmisPerYear = 0,
): ProjectionPoint[] {
  const plan = constructionPlanFor(inputs);
  const schedule = buildPaymentSchedule(inputs);
  const purchasePrice = schedule.reduce((sum, payment) => sum + payment.amount, 0);
  const emi = inputs.monthlyEmiThousands * 1_000;
  const loanRateMonthly = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const sipRateMonthly = inputs.equityReturn / 100 / MONTHS_IN_YEAR;
  const monthlySip = inputs.monthlySipThousands * 1_000;
  const startingRent = inputs.currentRentThousands * 1_000;
  const endMonth = inputs.holdingPeriodYears * MONTHS_IN_YEAR;
  const annualPrepayment = emi * Math.max(0, extraEmisPerYear);
  const paymentByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  const points: ProjectionPoint[] = [];

  // What each path commits per month. The baseline sets these equal, and the
  // buyer's surplus only appears once the loan stops consuming the EMI.
  const buyMonthlyBudget = emi;
  const rentMonthlyBudget = startingRent + monthlySip;

  // Buyer starts with the financed home, not an invented cash buffer.
  let buyInvestments = 0;
  let rentInvestments = 0;
  let loanBalance = 0;
  let builderPaid = 0;

  for (let month = 0; month <= endMonth; month += 1) {
    const payment = paymentByMonth.get(month);
    if (payment) {
      loanBalance += payment.loanAmount;
      builderPaid += payment.amount;
      // The rent path gets the same cash at the same time. Otherwise buying
      // would begin with down-payment equity while renting began at zero.
      rentInvestments += payment.cashAmount;
    }

    const hasPurchased = month >= plan.purchaseMonth;
    const hasPossession = month >= plan.possessionMonth;
    const monthlyRent = rentInMonth(startingRent, inputs.assumptions.rentInflationRate, month);
    const preEmiInterest = hasPurchased && !hasPossession
      ? loanBalance * loanRateMonthly
      : 0;
    const regularPayment = hasPossession && loanBalance > 0 && emi > 0
      ? Math.min(emi, loanBalance * (1 + loanRateMonthly))
      : 0;
    const monthlyBuyerHousingCost = hasPossession
      ? regularPayment
      : hasPurchased
        ? monthlyRent + preEmiInterest
        : 0;
    const propertyValue = compoundMonthly(
      inputs.propertyPriceLakh * LAKH,
      inputs.assumptions.homeAppreciationRate,
      month,
    );
    const builderBalance = hasPurchased ? Math.max(0, purchasePrice - builderPaid) : 0;

    if (month % MONTHS_IN_YEAR === 0) {
      points.push({
        year: month / MONTHS_IN_YEAR,
        buyNetWorth: hasPurchased
          ? propertyValue - loanBalance - builderBalance + buyInvestments
          : 0,
        rentNetWorth: rentInvestments,
        propertyValue,
        loanBalance,
        builderBalance,
        annualRent: monthlyRent * MONTHS_IN_YEAR,
        annualEmi: hasPossession && loanBalance > 0.5 ? emi * MONTHS_IN_YEAR : 0,
        monthlyBuyerHousingCost,
      });
    }

    if (month === endMonth) break;

    // Whatever the monthly commitment does not spend on housing is invested.
    buyInvestments = hasPurchased
      ? buyInvestments * (1 + sipRateMonthly)
        + Math.max(0, buyMonthlyBudget - monthlyBuyerHousingCost)
      : 0;
    rentInvestments = rentInvestments * (1 + sipRateMonthly)
      + Math.max(0, rentMonthlyBudget - monthlyRent);

    if (hasPossession && loanBalance > 0.5) {
      const interest = loanBalance * loanRateMonthly;
      loanBalance = Math.max(0, loanBalance + interest - regularPayment);
    }

    const paymentNumber = hasPossession ? month - plan.possessionMonth + 1 : 0;
    if (
      paymentNumber > 0
      && paymentNumber % MONTHS_IN_YEAR === 0
      && loanBalance > 0.5
      && annualPrepayment > 0
    ) {
      loanBalance = Math.max(0, loanBalance - Math.min(loanBalance, annualPrepayment));
    }
  }

  return points;
}
