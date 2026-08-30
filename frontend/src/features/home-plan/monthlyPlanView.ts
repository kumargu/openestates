import { formatCurrency, type PlanProjection, type ProjectionPoint } from "./model.ts";

export type MonthlyPlanVerdict = Readonly<{
  activeYear: number;
  activePoint: ProjectionPoint;
  buyWins: boolean;
  advantage: number;
  timeLabel: string;
  choiceLabel: "buy" | "rent";
  insight: string;
}>;

function yearLabel(year: number): string {
  if (year <= 0) return "today";
  return `${year} ${year === 1 ? "year" : "years"}`;
}

function sentenceYearLabel(year: number): string {
  if (year <= 0) return "Today";
  return `At ${yearLabel(year)}`;
}

function extraEmiLabel(extraEmisPerYear: number): string {
  if (extraEmisPerYear === 1) return "1 extra payment/year";
  return `${extraEmisPerYear} extra payments/year`;
}

function boundedYear(projection: PlanProjection, activeYear: number): number {
  const maxYear = Math.max(0, projection.points.length - 1);
  return Math.max(0, Math.min(activeYear, maxYear));
}

export function defaultPlanFocusYear(
  projection: Pick<PlanProjection, "loanFreeYear" | "points">,
  holdingPeriodYears: number,
): number {
  const maxYear = Math.max(0, projection.points.length - 1);
  return Math.min(holdingPeriodYears, maxYear);
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
  const choiceLabel = buyWins ? "buy" : "rent";
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
  const activePoint = projection.points[activeYear] ?? projection.points[0];
  const advantage = activePoint
    ? Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth)
    : 0;
  const lead = `${sentenceYearLabel(activeYear)}, ${buyWins ? "buying" : "renting"} leads by ${formatCurrency(advantage, true)}`;
  if (projection.loanFreeYear == null) {
    return `${lead}; loan does not close at this EMI.`;
  }

  if (projection.extraEmisPerYear > 0) {
    const interest = projection.totalInterest == null
      ? ""
      : ` Total interest lands near ${formatCurrency(projection.totalInterest, true)}.`;
    return `${lead}; ${extraEmiLabel(projection.extraEmisPerYear)} closes the loan in ${yearLabel(projection.loanFreeYear)}.${interest}`;
  }

  if (projection.breakEvenYear != null) {
    return `${lead}; break-even appears around ${yearLabel(projection.breakEvenYear)}.`;
  }

  return `${lead}; this stays ahead within ${yearLabel(horizonYears)}.`;
}
