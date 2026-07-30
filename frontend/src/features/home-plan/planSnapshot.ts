import type { NotebookLabelId } from "../../lib/notebook.ts";
import { formatCurrency, type PlanInputs, type PlanProjection } from "./model.ts";

type PlanSnapshotInput = {
  propertyId: string;
  propertyTitle: string;
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

function stableIndex(value: string, count: number): number {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
  }
  return hash % count;
}

export function buildPlanSnapshotNote({
  propertyId,
  propertyTitle,
  inputs,
  projection,
  activeYear,
}: PlanSnapshotInput): PlanSnapshotNote {
  const { boundedYear, point: activePoint } = activePointFor(projection, activeYear);
  const choice = activePoint.buyNetWorth >= activePoint.rentNetWorth ? "buying" : "renting and investing";
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const inspectedWindow = boundedYear <= 0 ? "today" : `after ${yearLabel(boundedYear)}`;
  const loanFree = loanFreeText(projection, inputs.holdingPeriodYears);
  const propertyPrice = formatCurrency(inputs.propertyPriceLakh * 100_000, true);
  const emi = formatCurrency(projection.monthlyEmi, true);
  const rent = formatCurrency(projection.monthlyRent, true);
  const sip = formatCurrency(projection.monthlySip, true);
  const homeValue = formatCurrency(activePoint.propertyValue, true);
  const title = `${propertyTitle} plan, ${emi} EMI`;
  const detailVariants = [
    `For ${propertyTitle} at ${propertyPrice}, this plan uses about ${emi} EMI with ${extraEmiText(projection.extraEmisPerYear)}; the ${loanFree}. The rent path is ${rent} rent plus ${sip} SIP, and ${inspectedWindow}, ${choice} is ahead by about ${formatCurrency(advantage, true)}. At that point the home is projected near ${homeValue}, so the note is mainly the property-value versus loan tradeoff.`,
    `${propertyTitle} was tested at ${propertyPrice} with a monthly EMI near ${emi}; with ${extraEmiText(projection.extraEmisPerYear)}, the ${loanFree}. Against that, renting uses ${rent} rent and ${sip} SIP, leaving ${choice} ahead by about ${formatCurrency(advantage, true)} ${inspectedWindow}. The home value reads near ${homeValue} in the same window.`,
    `This ${propertyTitle} plan keeps the buy side simple: ${propertyPrice} home, ${emi} EMI, ${extraEmiText(projection.extraEmisPerYear)}, and ${loanFree}. The other side is ${rent} rent with ${sip} SIP; ${inspectedWindow}, ${choice} leads by about ${formatCurrency(advantage, true)}. The home itself is projected near ${homeValue} then.`,
  ];
  const detail = detailVariants[stableIndex(`${propertyId}:${projection.monthlyEmi}:${projection.extraEmisPerYear}`, detailVariants.length)];

  return {
    catalogKey: `plan:${propertyId}:current`,
    title,
    detail,
    source: `Saved ${new Date().toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" })}`,
    labels: ["finance", "emi", "price"],
  };
}
