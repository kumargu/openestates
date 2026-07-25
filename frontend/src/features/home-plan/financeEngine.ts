import type {
  BuilderPayment,
  ConstructionProfile,
  PlanInputs,
  ProjectionPoint,
} from "./model.ts";

const LAKH = 100_000;
const MONTHS_IN_YEAR = 12;
const PAYMENT_INTERVAL_MONTHS = 6;
const DEFAULT_REMAINING_CONSTRUCTION_MONTHS = 24;
const DEFAULT_TOTAL_CONSTRUCTION_MONTHS = 36;
const MINIMUM_BOOKING_RATE = 0.1;
const PURCHASE_COST_RATE = 0.07;

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

function monthlyPayment(principal: number, annualRate: number, years: number): number {
  if (principal <= 0) return 0;
  const months = years * MONTHS_IN_YEAR;
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (monthlyRate === 0) return principal / months;
  const growth = (1 + monthlyRate) ** months;
  return principal * monthlyRate * growth / (growth - 1);
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
    inputs.appreciation,
    plan.purchaseMonth,
  );
  const requestedDownPayment = Math.min(inputs.downPaymentLakh * LAKH, purchasePrice);

  if (plan.possessionMonth === plan.purchaseMonth) {
    return [{
      month: plan.purchaseMonth,
      date: isoDate(plan.purchaseDate),
      amount: purchasePrice,
      cashAmount: requestedDownPayment,
      loanAmount: purchasePrice - requestedDownPayment,
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
  let cashRemaining = requestedDownPayment;
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
    const cashAmount = Math.min(cashRemaining, amount);
    cashRemaining -= cashAmount;

    return {
      month,
      date: isoDate(eventDate),
      amount,
      cashAmount,
      loanAmount: amount - cashAmount,
    };
  });
}

export function calculateFinancingInterest(inputs: PlanInputs): number {
  const plan = constructionPlanFor(inputs);
  const schedule = buildPaymentSchedule(inputs);
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const paymentsByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  let drawnLoan = 0;
  let preEmiInterest = 0;

  for (let month = plan.purchaseMonth; month < plan.possessionMonth; month += 1) {
    drawnLoan += paymentsByMonth.get(month)?.loanAmount ?? 0;
    preEmiInterest += drawnLoan * monthlyRate;
  }

  const totalLoan = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const emi = monthlyPayment(totalLoan, inputs.loanRate, inputs.loanTenureYears);
  return preEmiInterest + emi * inputs.loanTenureYears * MONTHS_IN_YEAR - totalLoan;
}

export function calculateProjectionPoints(inputs: PlanInputs): ProjectionPoint[] {
  const plan = constructionPlanFor(inputs);
  const schedule = buildPaymentSchedule(inputs);
  const purchasePrice = schedule.reduce((sum, payment) => sum + payment.amount, 0);
  const totalLoan = schedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const emi = monthlyPayment(totalLoan, inputs.loanRate, inputs.loanTenureYears);
  const loanRateMonthly = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const purchaseCosts = purchasePrice * PURCHASE_COST_RATE;
  const endMonth = inputs.holdingPeriodYears * MONTHS_IN_YEAR;
  const paymentByMonth = new Map(schedule.map((payment) => [payment.month, payment]));
  const points: ProjectionPoint[] = [];

  let buyerPortfolio = inputs.startingSavingsLakh * LAKH;
  let renterPortfolio = inputs.startingSavingsLakh * LAKH;
  let loanBalance = 0;
  let builderPaid = 0;
  let repaymentMonths = 0;
  let purchaseCostsPaid = false;

  for (let month = 0; month <= endMonth; month += 1) {
    const payment = paymentByMonth.get(month);
    if (payment) {
      if (!purchaseCostsPaid) {
        buyerPortfolio -= purchaseCosts;
        purchaseCostsPaid = true;
      }
      buyerPortfolio -= payment.cashAmount;
      loanBalance += payment.loanAmount;
      builderPaid += payment.amount;
    }

    const hasPurchased = month >= plan.purchaseMonth;
    const hasPossession = month >= plan.possessionMonth;
    const monthlyRent = compoundMonthly(
      inputs.currentRentThousands * 1_000,
      inputs.rentInflation,
      month,
    );
    const preEmiInterest = hasPurchased && !hasPossession
      ? loanBalance * loanRateMonthly
      : 0;
    const regularPayment = hasPossession && repaymentMonths < inputs.loanTenureYears * MONTHS_IN_YEAR
      ? Math.min(emi, loanBalance * (1 + loanRateMonthly))
      : 0;
    const monthlyBuyerHousingCost = hasPossession ? regularPayment : monthlyRent + preEmiInterest;
    const propertyValue = compoundMonthly(
      inputs.propertyPriceLakh * LAKH,
      inputs.appreciation,
      month,
    );
    const builderBalance = hasPurchased ? Math.max(0, purchasePrice - builderPaid) : 0;

    if (month % MONTHS_IN_YEAR === 0) {
      points.push({
        year: month / MONTHS_IN_YEAR,
        buyNetWorth: hasPurchased
          ? buyerPortfolio + propertyValue - loanBalance - builderBalance
          : buyerPortfolio,
        rentNetWorth: renterPortfolio,
        propertyValue,
        loanBalance,
        builderBalance,
        annualRent: monthlyRent * MONTHS_IN_YEAR,
        annualEmi: hasPossession ? emi * MONTHS_IN_YEAR : 0,
        monthlyBuyerHousingCost,
      });
    }

    if (month === endMonth) break;

    buyerPortfolio *= 1 + inputs.equityReturn / 100 / MONTHS_IN_YEAR;
    renterPortfolio *= 1 + inputs.equityReturn / 100 / MONTHS_IN_YEAR;

    if (monthlyBuyerHousingCost > monthlyRent) {
      renterPortfolio += monthlyBuyerHousingCost - monthlyRent;
    } else {
      buyerPortfolio += monthlyRent - monthlyBuyerHousingCost;
    }
    renterPortfolio += inputs.monthlyExtraInvestmentThousands * 1_000;

    if (hasPossession && regularPayment > 0) {
      const interest = loanBalance * loanRateMonthly;
      loanBalance = Math.max(0, loanBalance - Math.max(0, regularPayment - interest));
      repaymentMonths += 1;
    }
  }

  return points;
}
