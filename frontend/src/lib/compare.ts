import type { PropertyDetailResponse, ThemeLabel, ThemeResult, CompareThemes } from "./types.ts";

type PropData = PropertyDetailResponse["property"];
type AreaData = NonNullable<PropertyDetailResponse["area"]>;
type SocietyData = NonNullable<PropertyDetailResponse["society"]>;

function scoreToLabel(score: number): ThemeLabel {
  if (score >= 0.8) return "strong";
  if (score >= 0.6) return "good";
  if (score >= 0.4) return "mixed";
  return "weak";
}

function computeValue(p: PropData, area: AreaData | null): ThemeResult {
  if (!area) return { label: "mixed", summary: "Area data unavailable" };
  const ratio = p.price_per_sqft / area.median_price_per_sqft;
  const pctDiff = Math.round((1 - ratio) * 100);

  if (ratio < 0.9) {
    return { label: "strong", summary: `${Math.abs(pctDiff)}% below ${area.name} median — strong value` };
  } else if (ratio <= 1.05) {
    return { label: "good", summary: `Near ${area.name} median — fair market pricing` };
  } else if (ratio <= 1.2) {
    return { label: "mixed", summary: `${Math.abs(pctDiff)}% above ${area.name} median — premium pricing` };
  } else {
    return { label: "weak", summary: `${Math.abs(pctDiff)}% above ${area.name} median — significant premium` };
  }
}

function computeCommute(p: PropData): ThemeResult {
  const metro = p.metro_distance_mins;
  const traffic = p.traffic_score;

  if (metro <= 15 && traffic < 0.6) {
    return { label: "strong", summary: "Close metro access, manageable traffic" };
  } else if (metro <= 20 && traffic < 0.7) {
    return { label: "good", summary: `${metro} min to metro, moderate traffic` };
  } else if (metro <= 30) {
    return { label: "mixed", summary: `${metro} min to metro${traffic >= 0.7 ? ", heavy traffic area" : ""}` };
  } else {
    return { label: "weak", summary: `${metro} min to metro${traffic >= 0.7 ? ", congested corridor" : ", distant from metro"}` };
  }
}

function computeSociety(p: PropData, society: SocietyData | null): ThemeResult {
  const score = p.society_quality_score;
  const label = scoreToLabel(score);
  let summary: string;

  if (!society) {
    summary = score >= 0.8 ? "High quality society" : score >= 0.6 ? "Average society quality" : "Below average society quality";
  } else {
    const sentimentNote = society.maintenance_sentiment === "good" ? "well-maintained" : "average maintenance";
    if (score >= 0.85) {
      summary = `Premium society, ${sentimentNote}`;
    } else if (score >= 0.7) {
      summary = `Good society by ${society.builder_name}, ${sentimentNote}`;
    } else {
      summary = `Average society quality, ${sentimentNote}`;
    }
  }

  return { label, summary };
}

function computeGreenery(p: PropData): ThemeResult {
  const g = p.greenery_score ?? 0.5;
  const o = p.open_space_score ?? 0.5;
  const avg = (g + o) / 2;
  const label = scoreToLabel(avg);
  let summary: string;

  if (avg >= 0.8) {
    summary = "Lush greenery, spacious open areas";
  } else if (avg >= 0.65) {
    summary = "Good green cover and reasonable open space";
  } else if (avg >= 0.45) {
    summary = "Moderate greenery, denser built environment";
  } else {
    summary = "Limited greenery, dense surroundings";
  }

  return { label, summary };
}

function computeRisk(p: PropData): ThemeResult {
  const litigation = p.litigation_risk;
  const waterlogging = p.waterlogging_risk_score;
  const noise = p.noise_score;
  const avgRisk = (litigation + waterlogging + noise) / 3;

  // Lower risk score = better
  let label: ThemeLabel;
  let summary: string;
  const risks: string[] = [];

  if (litigation >= 0.15) risks.push("litigation concern");
  if (waterlogging >= 0.3) risks.push("waterlogging risk");
  if (noise >= 0.4) risks.push("elevated noise");

  if (avgRisk < 0.15) {
    label = "strong";
    summary = "Low risk across all dimensions";
  } else if (avgRisk < 0.25) {
    label = "good";
    summary = risks.length > 0 ? `Generally low risk — note: ${risks.join(", ")}` : "Low overall risk profile";
  } else if (avgRisk < 0.35) {
    label = "mixed";
    summary = risks.length > 0 ? `Mixed risk — ${risks.join(", ")}` : "Some risk factors to consider";
  } else {
    label = "weak";
    summary = `Elevated risk — ${risks.join(", ")}`;
  }

  return { label, summary };
}

function computeResale(p: PropData): ThemeResult {
  const score = p.resale_strength_score ?? 0.6;
  const label = scoreToLabel(score);
  let summary: string;

  if (score >= 0.85) {
    summary = "Strong resale demand, moves fast";
  } else if (score >= 0.7) {
    summary = "Good resale potential in this micro-market";
  } else if (score >= 0.5) {
    summary = "Average resale strength";
  } else {
    summary = "Weaker resale market — may take longer to sell";
  }

  return { label, summary };
}

function computeMarketTheme(p: PropData): ThemeResult {
  const interest = p.interest_level ?? "moderate";
  const saves = p.saves_last_7d ?? 0;
  const dom = p.days_on_market;

  if (interest === "high" && dom < 30) {
    return { label: "strong", summary: `High interest, ${saves} saves this week, ${dom} days on market` };
  } else if (interest === "high" || (interest === "moderate" && dom < 45)) {
    return { label: "good", summary: `${interest === "high" ? "High" : "Moderate"} interest, ${dom} days on market` };
  } else if (interest === "moderate") {
    return { label: "mixed", summary: `Moderate interest, ${dom} days on market` };
  } else {
    return { label: "weak", summary: `Low interest, ${dom} days on market` };
  }
}

export function computeThemes(
  p: PropData,
  area: AreaData | null,
  society: SocietyData | null,
): CompareThemes {
  return {
    value: computeValue(p, area),
    commute: computeCommute(p),
    society: computeSociety(p, society),
    greenery: computeGreenery(p),
    risk: computeRisk(p),
    resale: computeResale(p),
    market: computeMarketTheme(p),
  };
}

export function generateMatchSummary(
  p: PropData,
  area: AreaData | null,
): { headline: string; components: { label: string; level: ThemeLabel }[] } {
  const themes = computeThemes(p, area, null);

  // Build a contextual headline
  const strengths: string[] = [];
  const cautions: string[] = [];

  if (themes.value.label === "strong") strengths.push("strong value");
  else if (themes.value.label === "weak") cautions.push("premium pricing");

  if (themes.commute.label === "strong" || themes.commute.label === "good") strengths.push("good metro access");
  else if (themes.commute.label === "weak") cautions.push("commute tradeoff");

  if (themes.society.label === "strong") strengths.push("premium society");
  if (themes.greenery.label === "strong" || themes.greenery.label === "good") strengths.push("green surroundings");
  if (themes.risk.label === "weak" || themes.risk.label === "mixed") cautions.push("elevated risk signals");

  let headline: string;
  if (strengths.length >= 2) {
    headline = `Strong match for ${strengths.slice(0, 2).join(" and ")}.`;
  } else if (strengths.length === 1 && cautions.length > 0) {
    headline = `Good option for ${strengths[0]}, but ${cautions[0]}.`;
  } else if (strengths.length === 1) {
    headline = `Solid choice for ${strengths[0]}.`;
  } else if (cautions.length > 0) {
    headline = `Consider carefully — ${cautions.join(", ")}.`;
  } else {
    headline = "Balanced option across key dimensions.";
  }

  const components = [
    { label: "Value", level: themes.value.label },
    { label: "Commute & Access", level: themes.commute.label },
    { label: "Society Quality", level: themes.society.label },
    { label: "Greenery / Open Space", level: themes.greenery.label },
    { label: "Document Trust", level: scoreToLabel(p.document_completeness_score) },
    { label: "Risk Profile", level: (() => {
      // Invert: lower risk = better label
      const avg = (p.litigation_risk + p.waterlogging_risk_score + p.noise_score) / 3;
      if (avg < 0.15) return "strong" as ThemeLabel;
      if (avg < 0.25) return "good" as ThemeLabel;
      if (avg < 0.35) return "mixed" as ThemeLabel;
      return "weak" as ThemeLabel;
    })() },
  ];

  return { headline, components };
}

export function generateTradeoffs(
  p: PropData,
  area: AreaData | null,
): { strengths: string[]; cautions: string[] } {
  const strengths: string[] = [];
  const cautions: string[] = [];

  // Value
  if (area) {
    const ratio = p.price_per_sqft / area.median_price_per_sqft;
    if (ratio < 0.9) strengths.push(`Strong value relative to ${area.name} median`);
    else if (ratio > 1.1) cautions.push(`Premium pricing vs other ${area.name} listings`);
  }

  // Commute
  if (p.metro_distance_mins <= 18) strengths.push("Good metro access");
  else if (p.metro_distance_mins >= 35) cautions.push("Distant from metro");

  // Docs
  if (p.document_completeness_score >= 0.93) strengths.push("Strong documentation");
  else if (p.document_completeness_score < 0.85) cautions.push("Document completeness below standard — verify");

  // Society
  if (p.society_quality_score >= 0.85) strengths.push("Premium society quality");

  // Greenery
  const greenAvg = ((p.greenery_score ?? 0.5) + (p.open_space_score ?? 0.5)) / 2;
  if (greenAvg >= 0.75) strengths.push("Greener, more open surroundings");
  else if (greenAvg < 0.45) cautions.push("Dense built environment, limited greenery");

  // Traffic
  if (p.traffic_score >= 0.7) cautions.push("Heavier traffic in this corridor");

  // Litigation
  if (p.litigation_risk >= 0.15) cautions.push("Elevated litigation risk — engage a lawyer");

  // Waterlogging
  if (p.waterlogging_risk_score >= 0.3) cautions.push("Waterlogging risk — verify monsoon history");

  return { strengths: strengths.slice(0, 3), cautions: cautions.slice(0, 2) };
}

export function computeBestFor(
  properties: { id: string; title: string; themes: CompareThemes }[],
): { theme: string; propertyTitle: string; propertyId: string }[] {
  if (properties.length < 2) return [];

  const themeOrder: ThemeLabel[] = ["strong", "good", "mixed", "weak"];
  const themeKeys: { key: keyof CompareThemes; display: string }[] = [
    { key: "value", display: "Best for value" },
    { key: "commute", display: "Best for commute" },
    { key: "greenery", display: "Best for greenery" },
    { key: "society", display: "Best for society quality" },
    { key: "resale", display: "Best for resale" },
  ];

  const results: { theme: string; propertyTitle: string; propertyId: string }[] = [];

  for (const { key, display } of themeKeys) {
    let best = properties[0];
    for (const p of properties.slice(1)) {
      if (themeOrder.indexOf(p.themes[key].label) < themeOrder.indexOf(best.themes[key].label)) {
        best = p;
      }
    }
    // Only show if the winner is actually strong or good
    if (best.themes[key].label === "strong" || best.themes[key].label === "good") {
      results.push({ theme: display, propertyTitle: best.title, propertyId: best.id });
    }
  }

  return results;
}
