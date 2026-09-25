import labels from "../../../app/config/ui/inventory-preview.json";
import type { AskRange, HomeSummary, Observation } from "./generated/InventoryDetail.ts";

export { labels };
export function money(value: number): string {
  return value >= 10_000_000 ? `₹${(value / 10_000_000).toLocaleString("en-IN", { maximumFractionDigits: 2 })} Cr`
    : value >= 100_000 ? `₹${(value / 100_000).toLocaleString("en-IN", { maximumFractionDigits: 2 })} L`
    : `₹${value.toLocaleString("en-IN")}`;
}
export function askingPrice(ask: AskRange | null): string {
  if (!ask) return "Ask unavailable";
  if (ask.min_inr === ask.max_inr) return money(ask.min_inr);
  if (ask.min_inr >= 10_000_000) return `${money(ask.min_inr).replace(/ Cr$/, "")}–${money(ask.max_inr).replace(/^₹/, "")}`;
  return `${money(ask.min_inr)}–${money(ask.max_inr)}`;
}
export function clue(summary: HomeSummary): string {
  if (summary.price_change && !summary.uncertain_count && summary.active_count === summary.advertisement_count) {
    const change = summary.price_change.difference_inr;
    return `${money(Math.abs(change))} ${change < 0 ? "lower" : "higher"} in the same advertisement`;
  }
  const kind = summary.conflicts.length || summary.uncertain_count ? "uncertain"
    : summary.active_count === 0 ? "inactive"
    : summary.active_count < summary.advertisement_count ? "partial"
    : !summary.ask ? "missing"
    : summary.ask.min_inr !== summary.ask.max_inr ? "disagreement"
    : summary.advertisement_count > 1 ? "multiple" : "single";
  return labels.clues[kind].replace("{count}", String(summary.advertisement_count)).replace("{active}", String(summary.active_count));
}
export function dateLabel(value: string): string {
  const date = new Date(`${value}T12:00:00Z`);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat("en-IN", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" }).format(date);
}
export function physicalFacts(row: Pick<Observation, "bhk" | "area_sqft" | "area_basis" | "floor">): string[] {
  const basis = row.area_basis ? labels.areaBasis[row.area_basis as keyof typeof labels.areaBasis] : null;
  return [row.bhk ? `${row.bhk} BHK` : null, row.area_sqft ? `${row.area_sqft.toLocaleString("en-IN")} sqft${basis ? ` ${basis}` : ""}` : null, row.floor !== null ? `Floor ${row.floor}` : null].filter((value): value is string => Boolean(value));
}
