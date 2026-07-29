type IconFamily = "nearby" | "comparable" | "symbol";

export type LabelVisual = {
  title: string;
  classToken: string;
  family: IconFamily;
  icon: string;
  symbol?: string;
};

const LABEL_VISUALS: Record<string, LabelVisual> = {
  schools: { title: "Schools", classToken: "schools", family: "nearby", icon: "schools" },
  hospitals: { title: "Hospitals", classToken: "hospitals", family: "nearby", icon: "hospitals" },
  metro: { title: "Metro", classToken: "metro", family: "nearby", icon: "metro" },
  commute: { title: "Commute", classToken: "commute", family: "nearby", icon: "metro" },
  tech: { title: "Tech parks", classToken: "tech-parks", family: "nearby", icon: "tech" },
  tech_parks: { title: "Tech parks", classToken: "tech-parks", family: "nearby", icon: "tech" },
  breweries: { title: "Breweries", classToken: "breweries", family: "nearby", icon: "breweries" },
  water: { title: "Water", classToken: "water", family: "nearby", icon: "water" },
  cleanliness: { title: "Cleanliness", classToken: "community", family: "comparable", icon: "homeState" },
  location: { title: "Location", classToken: "commute", family: "nearby", icon: "essentials" },
  greenery: { title: "Greenery", classToken: "open-space", family: "comparable", icon: "openSpace" },
  condition: { title: "Condition", classToken: "risk", family: "comparable", icon: "builder" },
  open_space: { title: "Open space", classToken: "open-space", family: "comparable", icon: "openSpace" },
  "open-space": { title: "Open space", classToken: "open-space", family: "comparable", icon: "openSpace" },
  layout: { title: "Layout", classToken: "layout", family: "comparable", icon: "space" },
  approach: { title: "Approach road", classToken: "approach", family: "symbol", icon: "approach", symbol: "→" },
  risk: { title: "Risk", classToken: "risk", family: "nearby", icon: "red_flags" },
  transmission: { title: "High-tension line", classToken: "transmission", family: "nearby", icon: "red_flags" },
  price: { title: "Price proof", classToken: "price", family: "symbol", icon: "price", symbol: "₹" },
  emi: { title: "EMI", classToken: "emi", family: "symbol", icon: "emi", symbol: "₹" },
  finance: { title: "Finance", classToken: "emi", family: "symbol", icon: "finance", symbol: "₹" },
  down_payment: { title: "Down payment", classToken: "down-payment", family: "symbol", icon: "down-payment", symbol: "₹" },
  "down-payment": { title: "Down payment", classToken: "down-payment", family: "symbol", icon: "down-payment", symbol: "₹" },
  legal: { title: "Legal", classToken: "legal", family: "symbol", icon: "legal", symbol: "§" },
  community: { title: "Community", classToken: "community", family: "symbol", icon: "community", symbol: "“" },
  visit: { title: "Visit", classToken: "visit", family: "symbol", icon: "visit", symbol: "✓" },
  other: { title: "Other", classToken: "other", family: "nearby", icon: "essentials" },
};

function baseLabelId(id: string): string {
  return id.replace(/_under_\d+km$/, "").replace(/-under-\d+km$/, "");
}

export function labelVisual(id: string, fallbackTitle?: string): LabelVisual {
  const normalized = id.replace(/[^a-zA-Z0-9_-]/g, "_");
  const visual = LABEL_VISUALS[normalized] ?? LABEL_VISUALS[baseLabelId(normalized)];
  if (visual) return visual;
  return {
    title: fallbackTitle ?? id.replace(/[_-]/g, " "),
    classToken: normalized.replace(/_/g, "-"),
    family: "nearby",
    icon: "essentials",
  };
}

export function labelClassToken(id: string): string {
  return labelVisual(id).classToken;
}
