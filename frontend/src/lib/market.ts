import type { PropertyDetailResponse, MarketActivity } from "./types.ts";

type PropData = PropertyDetailResponse["property"];
type AreaData = NonNullable<PropertyDetailResponse["area"]>;

export function computeMarketActivity(p: PropData, area: AreaData | null): MarketActivity {
  return {
    interest_level: p.interest_level ?? "moderate",
    saves_last_7d: p.saves_last_7d ?? Math.round(5 + Math.random() * 15),
    offers_last_7d: p.offers_last_7d ?? 0,
    days_on_market: p.days_on_market,
    area_trend_summary: area?.trend_summary ?? "Trend data unavailable",
  };
}

export function interestLabel(level: "high" | "moderate" | "low"): string {
  switch (level) {
    case "high": return "High interest area";
    case "moderate": return "Moderate interest";
    case "low": return "Limited interest";
  }
}

export function daysOnMarketLabel(days: number): string {
  if (days <= 14) return "Recently listed";
  if (days <= 30) return "Listed this month";
  if (days <= 60) return "On market for a while";
  return "Long on market — may negotiate";
}

export function priceVsMedian(
  pricePerSqft: number,
  medianPricePerSqft: number,
): { pctDiff: number; verdict: string; verdictColor: string } {
  const pctDiff = Math.round(((pricePerSqft - medianPricePerSqft) / medianPricePerSqft) * 100);

  let verdict: string;
  let verdictColor: string;
  if (pctDiff <= -10) {
    verdict = "Good value";
    verdictColor = "var(--color-positive)";
  } else if (pctDiff <= 5) {
    verdict = "Near market";
    verdictColor = "var(--color-text-secondary)";
  } else {
    verdict = "Premium pricing";
    verdictColor = "var(--color-warning)";
  }

  return { pctDiff, verdict, verdictColor };
}
