import type { PlanControlSection } from "./PlanControls.tsx";
import type { PlanInputs, PlanProjection } from "./model.ts";
import { formatCurrency } from "./model.ts";

export type PlanControlField =
  | "downPaymentLakh"
  | "loanRate"
  | "appreciation"
  | "equityReturn"
  | "currentRentThousands"
  | "rentInflation";

export type AssumptionChip = {
  label: string;
  value: string;
  section: PlanControlSection;
  field: PlanControlField;
};

export type PlanMilestone = {
  year: number;
  label: string;
  definition: string;
};

export function assumptionChips(inputs: PlanInputs): AssumptionChip[] {
  return [
    { label: "Down payment", value: `₹${inputs.downPaymentLakh.toFixed(0)}L`, section: "financing", field: "downPaymentLakh" },
    { label: "Loan rate", value: `${inputs.loanRate.toFixed(1)}%`, section: "financing", field: "loanRate" },
    { label: "Home growth", value: `${inputs.appreciation.toFixed(1)}%`, section: "market", field: "appreciation" },
    { label: "Fund return", value: `${inputs.equityReturn.toFixed(1)}%`, section: "market", field: "equityReturn" },
    { label: "Rent", value: `₹${inputs.currentRentThousands.toFixed(0)}K/mo`, section: "market", field: "currentRentThousands" },
    { label: "Rent inflation", value: `${inputs.rentInflation.toFixed(1)}%/yr`, section: "market", field: "rentInflation" },
  ];
}

export function buildMilestones(
  purchaseYear: number,
  breakEvenYear: number | null,
  loanFreeYear: number | null,
): PlanMilestone[] {
  return [
    {
      year: purchaseYear,
      label: purchaseYear === 0 ? "Purchase" : `Buy in Y${purchaseYear}`,
      definition: purchaseYear === 0
        ? "You buy this home now and start the loan."
        : `You wait ${purchaseYear} years, then buy and start the loan.`,
    },
    ...(breakEvenYear !== null ? [{
      year: breakEvenYear,
      label: "Break-even",
      definition: "Home equity overtakes your rent + SIP portfolio.",
    }] : []),
    ...(loanFreeYear !== null ? [{
      year: loanFreeYear,
      label: "Loan free",
      definition: "Outstanding loan balance reaches zero.",
    }] : []),
  ];
}

export function describePlanChange(
  before: PlanProjection,
  after: PlanProjection,
  year: number,
): string | null {
  const notes: string[] = [];

  if (Math.round(before.monthlyEmi) !== Math.round(after.monthlyEmi)) {
    notes.push(`EMI is now ${formatCurrency(after.monthlyEmi)}/mo`);
  }

  if (before.breakEvenYear !== after.breakEvenYear) {
    if (after.breakEvenYear === null) notes.push("Buying does not catch up within 20 years");
    else if (before.breakEvenYear === null) notes.push(`Buying catches up in year ${after.breakEvenYear}`);
    else notes.push(`Break-even moved to year ${after.breakEvenYear}`);
  }

  const beforeGap = before.points[year].buyNetWorth - before.points[year].rentNetWorth;
  const afterGap = after.points[year].buyNetWorth - after.points[year].rentNetWorth;
  if (notes.length === 0 && Math.abs(afterGap - beforeGap) >= 50_000) {
    const leader = afterGap >= 0 ? "Buying" : "Rent + SIP";
    notes.push(`${leader} leads by ${formatCurrency(Math.abs(afterGap), true)} at year ${year}`);
  }

  return notes.length > 0 ? notes.join(" · ") : null;
}
