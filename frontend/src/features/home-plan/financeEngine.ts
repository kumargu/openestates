import type {
  BuilderPayment,
  ConstructionProfile,
  PlanInputs,
  ProjectionPoint,
} from "./model.ts";
import {
  DEFAULT_PLAN_MODEL_CONFIG,
  type PlanModelConfig,
  validatePlanModelConfig,
} from "./modelConfig.ts";

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
 * Under-construction homes use the configured builder-payment schedule.
 * Until possession the buyer pays rent + pre-EMI interest instead of EMI.
 */

const LAKH = 100_000;
const MONTHS_IN_YEAR = 12;

export type RepaymentStrategy = "finish_earlier" | "lower_emi";
export type LoanRepaymentStatus =
  | "no_loan"
  | "repaid"
  | "insufficient_emi"
  | "simulation_limit";

export type LoanScheduleMonth = {
  month: number;
  paymentNumber: number;
  openingBalance: number;
  scheduledEmi: number;
  scheduledPayment: number;
  interestPaid: number;
  principalPaid: number;
  extraPaid: number;
  closingBalance: number;
};

export type LoanSchedule = {
  months: LoanScheduleMonth[];
  openingMonthlyEmi: number;
  endingMonthlyEmi: number;
  /** First extra payment. Later lower-EMI extras can be smaller. */
  annualPrepayment: number;
  baselinePayoffMonth: number | null;
  payoffMonth: number | null;
  totalInterest: number | null;
  status: LoanRepaymentStatus;
};

export type LoanScheduleOptions = {
  extraEmisPerYear?: number;
  strategy?: RepaymentStrategy;
  /** Restricts extras to one repayment year; omitted means every year. */
  oneOffExtraPaymentYear?: number;
  /** Starts recurring annual extras in this repayment year. */
  extraEmisStartYear?: number;
};

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

function monthlyPaymentForMonths(
  principal: number,
  annualRate: number,
  months: number,
): number {
  if (principal <= 0) return 0;
  if (months <= 0) return Number.POSITIVE_INFINITY;
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

/**
 * Canonical monthly loan schedule used by repayment, interest and wealth views.
 * Each annual extra is a multiple of the EMI scheduled in that repayment year.
 * This matters for lower-EMI plans: future extras fall with the re-amortised EMI.
 */
export function buildLoanSchedule(
  inputs: PlanInputs,
  options: LoanScheduleOptions = {},
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): LoanSchedule {
  validatePlanModelConfig(config);
  const extraEmisPerYear = options.extraEmisPerYear ?? config.defaults.extraEmisPerYear;
  if (!Number.isFinite(extraEmisPerYear) || extraEmisPerYear < 0 || !Number.isInteger(extraEmisPerYear)) {
    throw new RangeError("extraEmisPerYear must be a finite whole number >= 0");
  }
  if (
    options.oneOffExtraPaymentYear != null
    && (
      !Number.isFinite(options.oneOffExtraPaymentYear)
      || options.oneOffExtraPaymentYear < 1
      || !Number.isInteger(options.oneOffExtraPaymentYear)
    )
  ) {
    throw new RangeError("oneOffExtraPaymentYear must be a finite whole number >= 1");
  }
  if (
    options.extraEmisStartYear != null
    && (
      !Number.isFinite(options.extraEmisStartYear)
      || options.extraEmisStartYear < 1
      || !Number.isInteger(options.extraEmisStartYear)
    )
  ) {
    throw new RangeError("extraEmisStartYear must be a finite whole number >= 1");
  }
  if (options.oneOffExtraPaymentYear != null && options.extraEmisStartYear != null) {
    throw new RangeError("one-off and recurring-start timing cannot be combined");
  }
  const strategy = options.strategy ?? "finish_earlier";
  const plan = constructionPlanFor(inputs, config);
  const builderSchedule = buildPaymentSchedule(inputs, config);
  const drawsByMonth = new Map(builderSchedule.map((payment) => [payment.month, payment.loanAmount]));
  const principal = builderSchedule.reduce((sum, payment) => sum + payment.loanAmount, 0);
  const openingMonthlyEmi = inputs.monthlyEmiThousands * 1_000;
  const baselineRepaymentMonths = monthsToPayoff(principal, inputs.loanRate, openingMonthlyEmi);
  const baselinePayoffMonth = Number.isFinite(baselineRepaymentMonths)
    ? plan.possessionMonth + baselineRepaymentMonths
    : null;
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const insufficientEmi = (
    principal > config.simulation.closedBalanceRupees
    && monthlyRate > 0
    && openingMonthlyEmi <= principal * monthlyRate
  );
  const maximumMonth = plan.possessionMonth + config.simulation.maximumLoanYears * MONTHS_IN_YEAR;
  const months: LoanScheduleMonth[] = [];
  let balance = 0;
  let currentMonthlyEmi = openingMonthlyEmi;
  let totalInterest = 0;
  let payoffMonth: number | null = principal <= config.simulation.closedBalanceRupees
    ? plan.possessionMonth
    : null;

  for (let month = 0; month < maximumMonth && payoffMonth == null; month += 1) {
    balance += drawsByMonth.get(month) ?? 0;
    const openingBalance = balance;
    const hasPossession = month >= plan.possessionMonth;
    const paymentNumber = hasPossession ? month - plan.possessionMonth + 1 : 0;
    const interestPaid = balance * monthlyRate;
    let scheduledPayment = hasPossession
      ? Math.min(currentMonthlyEmi, balance + interestPaid)
      : interestPaid;
    let principalPaid = hasPossession ? Math.max(0, scheduledPayment - interestPaid) : 0;
    let extraPaid = 0;

    if (hasPossession) {
      balance = Math.max(0, balance + interestPaid - scheduledPayment);
      const paymentYear = Math.ceil(paymentNumber / MONTHS_IN_YEAR);
      const extraPaymentIsActive = options.oneOffExtraPaymentYear == null
        ? paymentYear >= (options.extraEmisStartYear ?? 1)
        : paymentYear === options.oneOffExtraPaymentYear;
      const scheduledAnnualExtra = currentMonthlyEmi * extraEmisPerYear;
      const canPreserveBaselinePayoff = (
        strategy !== "lower_emi"
        || baselinePayoffMonth == null
        || month + 1 < baselinePayoffMonth
      );
      if (
        paymentNumber % MONTHS_IN_YEAR === 0
        && balance > config.simulation.closedBalanceRupees
        && scheduledAnnualExtra > 0
        && extraPaymentIsActive
        && canPreserveBaselinePayoff
      ) {
        const maximumExtra = strategy === "lower_emi"
          ? Math.max(0, balance - config.simulation.closedBalanceRupees)
          : balance;
        extraPaid = Math.min(maximumExtra, scheduledAnnualExtra);
        balance -= extraPaid;
      }
    }

    totalInterest += interestPaid;
    const scheduledEmi = currentMonthlyEmi;
    if (
      strategy === "lower_emi"
      && extraPaid > 0
      && baselinePayoffMonth != null
      && balance > config.simulation.closedBalanceRupees
    ) {
      const remainingPayments = baselinePayoffMonth - (month + 1);
      currentMonthlyEmi = monthlyPaymentForMonths(balance, inputs.loanRate, remainingPayments);
    }

    if (!hasPossession) {
      scheduledPayment = interestPaid;
      principalPaid = 0;
    }
    if (hasPossession && balance <= config.simulation.closedBalanceRupees) {
      balance = 0;
      payoffMonth = month + 1;
    }

    months.push({
      month,
      paymentNumber,
      openingBalance,
      scheduledEmi,
      scheduledPayment,
      interestPaid,
      principalPaid,
      extraPaid,
      closingBalance: balance,
    });
  }

  return {
    months,
    openingMonthlyEmi,
    endingMonthlyEmi: currentMonthlyEmi,
    annualPrepayment: months.find((month) => month.extraPaid > 0)?.extraPaid ?? 0,
    baselinePayoffMonth,
    payoffMonth,
    totalInterest: payoffMonth == null ? null : totalInterest,
    status: principal <= config.simulation.closedBalanceRupees
      ? "no_loan"
      : payoffMonth != null
        ? "repaid"
        : insufficientEmi
          ? "insufficient_emi"
          : "simulation_limit",
  };
}

export function constructionPlanFor(
  inputs: PlanInputs,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): ConstructionPlan {
  validatePlanModelConfig(config);
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
    : addMonths(purchaseDate, config.construction.estimatedRemainingMonths);
  const suppliedStart = parsePlanDate(inputs.construction.startDate);
  const startDate = suppliedStart && suppliedStart < possessionDateValue
    ? suppliedStart
    : addMonths(possessionDateValue, -config.construction.estimatedTotalMonths);
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

export function buildPaymentSchedule(
  inputs: PlanInputs,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
): BuilderPayment[] {
  const plan = constructionPlanFor(inputs, config);
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
    let month = plan.purchaseMonth + config.construction.paymentIntervalMonths;
    month < plan.possessionMonth;
    month += config.construction.paymentIntervalMonths
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
      ? Math.min(1, Math.max(config.construction.minimumBookingPercent / 100, elapsedAtPurchase))
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
  extraEmisPerYear?: number,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
  strategy: RepaymentStrategy = "finish_earlier",
): number | null {
  return buildLoanSchedule(inputs, {
    extraEmisPerYear: extraEmisPerYear ?? config.defaults.extraEmisPerYear,
    strategy,
  }, config).totalInterest;
}

export function calculateProjectionPoints(
  inputs: PlanInputs,
  extraEmisPerYear?: number,
  config: PlanModelConfig = DEFAULT_PLAN_MODEL_CONFIG,
  strategy: RepaymentStrategy = "finish_earlier",
): ProjectionPoint[] {
  extraEmisPerYear ??= config.defaults.extraEmisPerYear;
  const plan = constructionPlanFor(inputs, config);
  const schedule = buildPaymentSchedule(inputs, config);
  const loanSchedule = buildLoanSchedule(inputs, { extraEmisPerYear, strategy }, config);
  const loanMonths = new Map(loanSchedule.months.map((month) => [month.month, month]));
  const purchasePrice = schedule.reduce((sum, payment) => sum + payment.amount, 0);
  const emi = inputs.monthlyEmiThousands * 1_000;
  const sipRateMonthly = inputs.equityReturn / 100 / MONTHS_IN_YEAR;
  const monthlySip = inputs.monthlySipThousands * 1_000;
  const startingRent = inputs.currentRentThousands * 1_000;
  const endMonth = inputs.holdingPeriodYears * MONTHS_IN_YEAR;
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
      builderPaid += payment.amount;
      // The rent path gets the same cash at the same time. Otherwise buying
      // would begin with down-payment equity while renting began at zero.
      rentInvestments += payment.cashAmount;
    }

    const loanMonth = loanMonths.get(month);
    if (loanMonth) loanBalance = loanMonth.openingBalance;

    const hasPurchased = month >= plan.purchaseMonth;
    const hasPossession = month >= plan.possessionMonth;
    const monthlyRent = rentInMonth(startingRent, inputs.assumptions.rentInflationRate, month);
    const preEmiInterest = hasPurchased && !hasPossession
      ? loanMonth?.interestPaid ?? 0
      : 0;
    const regularPayment = hasPossession ? loanMonth?.scheduledPayment ?? 0 : 0;
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
        annualEmi: hasPossession && loanBalance > config.simulation.closedBalanceRupees
          ? (loanMonth?.scheduledEmi ?? emi) * MONTHS_IN_YEAR
          : 0,
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

    if (loanMonth) loanBalance = loanMonth.closingBalance;
  }

  return points;
}
