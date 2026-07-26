export type PlanInputs = {
  propertyPriceLakh: number;
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
};

export type ProjectionPoint = {
  year: number;
  buyNetWorth: number;
  rentNetWorth: number;
  propertyValue: number;
  loanBalance: number;
  annualRent: number;
  annualEmi: number;
};

export type PlanProjection = {
  monthlyEmi: number;
  loanAmount: number;
  totalInterest: number;
  breakEvenYear: number | null;
  liquidityAfterDownPayment: number;
  opportunityCost: number;
  points: ProjectionPoint[];
  sensitivity: SensitivityCell[];
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

function remainingBalance(
  principal: number,
  annualRate: number,
  years: number,
  elapsedYears: number,
): number {
  const totalMonths = years * MONTHS_IN_YEAR;
  const paidMonths = Math.min(elapsedYears * MONTHS_IN_YEAR, totalMonths);
  const monthlyRate = annualRate / 100 / MONTHS_IN_YEAR;
  if (paidMonths >= totalMonths) return 0;
  if (monthlyRate === 0) return principal * (1 - paidMonths / totalMonths);
  const payment = monthlyPayment(principal, annualRate, years);
  const growth = (1 + monthlyRate) ** paidMonths;
  return principal * growth - payment * ((growth - 1) / monthlyRate);
}

export function calculateLoanJourney(
  inputs: PlanInputs,
  extraEmisPerYear: number,
): LoanJourney {
  const propertyPrice = inputs.propertyPriceLakh * LAKH;
  const downPayment = inputs.downPaymentLakh * LAKH;
  const purchasePrice = compound(propertyPrice, inputs.appreciation, inputs.purchaseYear);
  const principal = Math.max(0, purchasePrice - downPayment);
  const monthlyEmi = monthlyPayment(principal, inputs.loanRate, inputs.loanTenureYears);
  const totalMonths = inputs.loanTenureYears * MONTHS_IN_YEAR;
  const monthlyRate = inputs.loanRate / 100 / MONTHS_IN_YEAR;
  const annualPrepayment = monthlyEmi * extraEmisPerYear;
  const points: LoanJourneyPoint[] = [{ year: 0, balance: principal, interestPaid: 0, principalPaid: 0, extraPaid: 0 }];

  let balance = principal;
  let month = 0;
  let totalInterest = 0;
  let yearlyInterest = 0;
  let yearlyPrincipal = 0;
  let yearlyExtra = 0;

  while (balance > 0.5 && month < totalMonths) {
    month += 1;
    const interest = balance * monthlyRate;
    const regularPayment = Math.min(monthlyEmi, balance + interest);
    const principalPayment = Math.max(0, regularPayment - interest);
    balance = Math.max(0, balance - principalPayment);
    totalInterest += interest;
    yearlyInterest += interest;
    yearlyPrincipal += principalPayment;

    if (month % MONTHS_IN_YEAR === 0 && balance > 0 && extraEmisPerYear > 0) {
      yearlyExtra = Math.min(balance, annualPrepayment);
      balance -= yearlyExtra;
    }

    if (month % MONTHS_IN_YEAR === 0 || balance <= 0.5) {
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

  for (let year = points.at(-1)?.year ?? 0; year < inputs.loanTenureYears; year += 1) {
    points.push({ year: year + 1, balance: 0, interestPaid: 0, principalPaid: 0, extraPaid: 0 });
  }

  const originalInterest = monthlyEmi * totalMonths - principal;

  return {
    monthlyEmi,
    annualPrepayment,
    loanFreeMonths: month,
    monthsSaved: Math.max(0, totalMonths - month),
    interestSaved: Math.max(0, originalInterest - totalInterest),
    totalInterest,
    points,
  };
}

function calculatePoints(inputs: PlanInputs): ProjectionPoint[] {
  const propertyPrice = inputs.propertyPriceLakh * LAKH;
  const downPayment = inputs.downPaymentLakh * LAKH;
  const purchasePrice = compound(propertyPrice, inputs.appreciation, inputs.purchaseYear);
  const loanAmount = Math.max(0, purchasePrice - downPayment);
  const monthlyEmi = monthlyPayment(loanAmount, inputs.loanRate, inputs.loanTenureYears);
  const startingRent = inputs.currentRentThousands * 1_000;
  const purchaseCosts = purchasePrice * PURCHASE_COST_RATE;
  const points: ProjectionPoint[] = [];

  let rentPortfolio = STARTING_LIQUID_SAVINGS_LAKH * LAKH;
  let buyerPortfolio = STARTING_LIQUID_SAVINGS_LAKH * LAKH;
  let purchaseCompleted = false;

  for (let year = 0; year <= inputs.holdingPeriodYears; year += 1) {
    if (!purchaseCompleted && year >= inputs.purchaseYear) {
      buyerPortfolio = Math.max(0, buyerPortfolio - downPayment - purchaseCosts);
      purchaseCompleted = true;
    }

    const hasPurchased = year >= inputs.purchaseYear;
    const loanElapsedYears = Math.max(0, year - inputs.purchaseYear);
    const propertyValue = compound(propertyPrice, inputs.appreciation, year);
    const loanBalance = hasPurchased
      ? remainingBalance(loanAmount, inputs.loanRate, inputs.loanTenureYears, loanElapsedYears)
      : 0;
    const monthlyRent = compound(startingRent, inputs.rentInflation, year);
    const annualRent = monthlyRent * MONTHS_IN_YEAR;
    const annualEmi = hasPurchased && loanElapsedYears < inputs.loanTenureYears
      ? monthlyEmi * MONTHS_IN_YEAR
      : 0;

    points.push({
      year,
      buyNetWorth: hasPurchased ? propertyValue - loanBalance + buyerPortfolio : buyerPortfolio,
      rentNetWorth: rentPortfolio,
      propertyValue,
      loanBalance,
      annualRent,
      annualEmi,
    });

    if (year < inputs.holdingPeriodYears) {
      const returnFactor = 1 + inputs.equityReturn / 100;
      buyerPortfolio *= returnFactor;
      rentPortfolio *= returnFactor;
      const monthlyDifference = Math.max(0, annualEmi / MONTHS_IN_YEAR - monthlyRent);
      const monthlyExtraInvestment = inputs.monthlyExtraInvestmentThousands * 1_000;
      rentPortfolio += (monthlyDifference + monthlyExtraInvestment) * MONTHS_IN_YEAR;
    }
  }

  return points;
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
  const totalInterest = monthlyEmi * inputs.loanTenureYears * MONTHS_IN_YEAR - loanAmount;
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
    STARTING_LIQUID_SAVINGS_LAKH * LAKH,
    inputs.equityReturn,
    inputs.purchaseYear,
  );

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
  downPaymentLakh: 40,
  loanRate: 8.4,
  loanTenureYears: 20,
  currentRentThousands: 55,
  rentInflation: 6,
  appreciation: 6.5,
  equityReturn: 10,
  monthlyExtraInvestmentThousands: 0,
  holdingPeriodYears: 15,
  purchaseYear: 0,
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
