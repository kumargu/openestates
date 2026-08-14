import type { BuyerProofCoverageGap, BuyerProofReceipt } from "./types.ts";

export function buyerProofReceiptLabel(receipt: BuyerProofReceipt): string {
  if (typeof receipt.distance_m !== "number") return receipt.label;
  return `${receipt.label} · ${formatBuyerProofDistance(receipt.distance_m)}`;
}

export function buyerProofCoverageLabel(gap: BuyerProofCoverageGap): string {
  const label = gap.preference.trim().replace(/\s+/g, " ");
  const sentenceLabel = label
    ? `${label.charAt(0).toLocaleUpperCase("en-IN")}${label.slice(1)}`
    : "This preference";
  return gap.status === "conflicted"
    ? `${sentenceLabel} has conflicting evidence`
    : `${sentenceLabel} not yet verified`;
}

function formatBuyerProofDistance(distanceM: number): string {
  return distanceM < 1000
    ? `${Math.round(distanceM)} m`
    : `${(distanceM / 1000).toFixed(1)} km`;
}
