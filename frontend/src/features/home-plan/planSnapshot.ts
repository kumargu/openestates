import type { NotebookLabelId } from "../../lib/notebook.ts";
import { formatCurrency, type PlanInputs, type PlanProjection, type ProjectionPoint } from "./model.ts";

type PlanSnapshotInput = {
  propertyId: string;
  inputs: PlanInputs;
  projection: PlanProjection;
  activeYear: number;
  activePoint: ProjectionPoint;
  extraEmisPerYear: number;
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

function rateLabel(value: number): string {
  return `${Number(value.toFixed(1)).toLocaleString("en-IN")}%`;
}

function roundedKeyValue(value: number): string {
  return Math.round(value).toString(36);
}

function loanFreeText(projection: PlanProjection, horizonYears: number): string {
  if (projection.loanFreeYear == null) {
    return `loan is still open after ${yearLabel(horizonYears)}`;
  }
  return `loan closes in ${yearLabel(projection.loanFreeYear)}`;
}

export function buildPlanSnapshotNote({
  propertyId,
  inputs,
  projection,
  activeYear,
  activePoint,
  extraEmisPerYear,
}: PlanSnapshotInput): PlanSnapshotNote {
  const choice = activePoint.buyNetWorth >= activePoint.rentNetWorth ? "buying" : "renting and investing";
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const loanFree = loanFreeText(projection, inputs.holdingPeriodYears);
  const inspectedWindow = activeYear <= 0 ? "today" : `over ${yearLabel(activeYear)}`;
  const title = `${formatCurrency(projection.monthlyEmi, true)} EMI, ${loanFree}`;
  const rentPath = `${formatCurrency(projection.monthlyRent, true)} rent + ${formatCurrency(projection.monthlySip, true)} SIP`;
  const extraEmiText = extraEmisPerYear === 1
    ? "1 extra EMI/year"
    : `${extraEmisPerYear} extra EMIs/year`;
  const detail = [
    `With ${formatCurrency(projection.monthlyEmi, true)} monthly EMI and ${extraEmiText}, ${loanFree}.`,
    `At the inspected point ${inspectedWindow}, ${choice} is ahead by ${formatCurrency(advantage, true)}.`,
    `The rent path uses ${rentPath} and reaches ${formatCurrency(activePoint.rentNetWorth, true)} net worth; the home is projected at ${formatCurrency(activePoint.propertyValue, true)}.`,
    `Assumes ${rateLabel(inputs.loanRate)} loan rate and ${rateLabel(inputs.equityReturn)} SIP return.`,
  ].join(" ");

  return {
    catalogKey: [
      "plan",
      propertyId,
      activeYear,
      roundedKeyValue(projection.monthlyEmi),
      extraEmisPerYear,
      roundedKeyValue(projection.monthlyRent),
      roundedKeyValue(projection.monthlySip),
      rateLabel(inputs.loanRate),
      rateLabel(inputs.equityReturn),
    ].join(":"),
    title,
    detail,
    source: "Plan snapshot",
    labels: ["finance", "emi", "down-payment", "price"],
  };
}
