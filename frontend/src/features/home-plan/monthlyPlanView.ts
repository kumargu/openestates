import { formatCurrency, type PlanProjection, type ProjectionPoint } from "./model.ts";

export type MonthlyPlanVerdict = Readonly<{
  activeYear: number;
  activePoint: ProjectionPoint;
  buyWins: boolean;
  advantage: number;
  timeLabel: string;
  choiceLabel: "buy" | "rent and invest";
  insight: string;
}>;

function yearLabel(year: number): string {
  if (year <= 0) return "today";
  return `${year} ${year === 1 ? "year" : "years"}`;
}

function extraEmiLabel(extraEmisPerYear: number): string {
  if (extraEmisPerYear === 1) return "1 extra EMI/year";
  return `${extraEmisPerYear} extra EMIs/year`;
}

function boundedYear(projection: PlanProjection, activeYear: number): number {
  const maxYear = Math.max(0, projection.points.length - 1);
  return Math.max(0, Math.min(activeYear, maxYear));
}

export function buildMonthlyPlanVerdict(
  projection: PlanProjection,
  activeYear: number,
): MonthlyPlanVerdict {
  const inspectedYear = boundedYear(projection, activeYear);
  const activePoint = projection.points[inspectedYear] ?? projection.points[0];
  if (!activePoint) {
    throw new RangeError("projection must include at least one point");
  }

  const buyWins = activePoint.buyNetWorth >= activePoint.rentNetWorth;
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const choiceLabel = buyWins ? "buy" : "rent and invest";
  const timeLabel = inspectedYear === 0
    ? "Today"
    : `After ${inspectedYear} ${inspectedYear === 1 ? "year" : "years"}`;
  const insight = monthlyPlanInsight(projection, inspectedYear, buyWins);

  return {
    activeYear: inspectedYear,
    activePoint,
    buyWins,
    advantage,
    timeLabel,
    choiceLabel,
    insight,
  };
}

export function monthlyPlanInsight(
  projection: PlanProjection,
  activeYear: number,
  buyWins: boolean,
): string {
  const horizonYears = Math.max(0, projection.points.length - 1);
  if (projection.loanFreeYear == null) {
    return "Loan does not close at this EMI.";
  }

  if (projection.extraEmisPerYear > 0) {
    const interest = projection.totalInterest == null
      ? ""
      : ` Total interest lands near ${formatCurrency(projection.totalInterest, true)}.`;
    return `${extraEmiLabel(projection.extraEmisPerYear)} closes the loan in ${yearLabel(projection.loanFreeYear)}.${interest}`;
  }

  if (projection.breakEvenYear != null) {
    return `Break-even appears around ${yearLabel(projection.breakEvenYear)}; this view is reading ${yearLabel(activeYear)}.`;
  }

  const choice = buyWins ? "buying" : "renting and investing";
  return `Within ${yearLabel(horizonYears)}, ${choice} stays ahead at the inspected year.`;
}
