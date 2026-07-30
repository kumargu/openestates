import type { NotebookLabelId } from "../../lib/notebook.ts";
import { formatCurrency, type PlanInputs, type PlanProjection } from "./model.ts";

type PlanSnapshotInput = {
  propertyId: string;
  inputs: PlanInputs;
  projection: PlanProjection;
  activeYear: number;
};

export type PlanSnapshotNote = {
  catalogKey: string;
  title: string;
  detail: string;
  source: string;
  labels: NotebookLabelId[];
};

function yearLabel(year: number): string {
  if (year <= 0) return "today";
  return `${year} ${year === 1 ? "year" : "years"}`;
}

function activePointFor(projection: PlanProjection, activeYear: number) {
  const boundedYear = Math.max(0, Math.min(activeYear, projection.points.length - 1));
  const point = projection.points[boundedYear];
  if (!point) throw new RangeError("projection must include at least one point");
  return { boundedYear, point };
}

function loanFreeText(projection: PlanProjection, horizonYears: number): string {
  if (projection.loanFreeYear == null) {
    return `loan still open after ${yearLabel(horizonYears)}`;
  }
  return `loan closes in ${yearLabel(projection.loanFreeYear)}`;
}

function extraEmiText(extraEmisPerYear: number): string {
  if (extraEmisPerYear === 1) return "1 extra EMI/year";
  return `${extraEmisPerYear} extra EMIs/year`;
}

export function buildPlanSnapshotNote({
  propertyId,
  inputs,
  projection,
  activeYear,
}: PlanSnapshotInput): PlanSnapshotNote {
  const { boundedYear, point: activePoint } = activePointFor(projection, activeYear);
  const choice = activePoint.buyNetWorth >= activePoint.rentNetWorth ? "buying" : "renting and investing";
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const inspectedWindow = boundedYear <= 0 ? "today" : `after ${yearLabel(boundedYear)}`;
  const loanFree = loanFreeText(projection, inputs.holdingPeriodYears);
  const title = `${formatCurrency(projection.monthlyEmi, true)} EMI, ${loanFree}`;
  const detail = [
    `Monthly plan: ${formatCurrency(projection.monthlyEmi, true)} EMI with ${extraEmiText(projection.extraEmisPerYear)}.`,
    `Rent path: ${formatCurrency(projection.monthlyRent, true)} rent and ${formatCurrency(projection.monthlySip, true)} SIP.`,
    `${inspectedWindow}, ${choice} is ahead by about ${formatCurrency(advantage, true)}.`,
    `Assumptions: ${inputs.loanRate}% loan, ${inputs.equityReturn}% SIP return, ${projection.assumptions.homeAppreciationRate}% home growth, ${projection.assumptions.rentInflationRate}% rent growth.`,
  ].join(" ");

  return {
    catalogKey: `plan:${propertyId}:current`,
    title,
    detail,
    source: `Saved ${new Date().toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" })}`,
    labels: ["finance", "emi", "price"],
  };
}
