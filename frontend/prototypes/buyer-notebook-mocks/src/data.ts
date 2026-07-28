export type PropertyId = "waterford" | "dream-acres" | "park-retreat";

export type NoteKind = "fact" | "theme" | "plan" | "handwritten" | "question";

export type NoteMark = "fact" | "concern" | "question" | "note" | "money";

export type TagId =
  | "down-payment"
  | "emi"
  | "schools"
  | "water"
  | "layout"
  | "legal"
  | "price"
  | "commute"
  | "open-space"
  | "visit";

export type TagDef = {
  id: TagId;
  label: string;
  color: string; // soft pill background
  ink: string;
};

export const TAGS: TagDef[] = [
  { id: "down-payment", label: "Down payment", color: "#e8deee", ink: "#5b3b6e" },
  { id: "emi", label: "EMI", color: "#ddebf5", ink: "#2b5c7a" },
  { id: "schools", label: "Schools", color: "#d6e9db", ink: "#2d5a3d" },
  { id: "water", label: "Water", color: "#f5e0c8", ink: "#7a4a12" },
  { id: "layout", label: "Layout", color: "#f1e2d8", ink: "#7a3f28" },
  { id: "legal", label: "Legal", color: "#e3e4f5", ink: "#3d3f7a" },
  { id: "price", label: "Price proof", color: "#f5e6ea", ink: "#7a2f45" },
  { id: "commute", label: "Commute", color: "#e2eee8", ink: "#2f5c4a" },
  { id: "open-space", label: "Open space", color: "#e8f0d8", ink: "#4a5c28" },
  { id: "visit", label: "Visit", color: "#ececec", ink: "#444" },
];

export type CatalogFact = {
  id: string;
  propertyId: PropertyId;
  kind: Exclude<NoteKind, "handwritten" | "question">;
  mark: NoteMark;
  tag: TagId;
  label: string;
  detail?: string;
  source?: string;
};

export type PropertySeed = {
  id: PropertyId;
  name: string;
  short: string;
  area: string;
  bhk: string;
  sqft: number;
  priceCr: number;
  whyHere: string[];
  hero: string;
  icon: string;
};

export const MONEY = {
  downPaymentL: 58,
  comfortableEmiL: 1.35,
  ceilingEmiL: 1.55,
  currentRent: 48000,
};

export const PROPERTIES: PropertySeed[] = [
  {
    id: "waterford",
    name: "Prestige Waterford",
    short: "Waterford",
    area: "Whitefield",
    bhk: "3 BHK",
    sqft: 1780,
    priceCr: 2.45,
    whyHere: ["Best school access", "Strong layout"],
    hero: "School road on the same stretch — daughter would not sit an hour in a bus.",
    icon: "🏫",
  },
  {
    id: "dream-acres",
    name: "Sobha Dream Acres",
    short: "Dream Acres",
    area: "Panathur",
    bhk: "3 BHK",
    sqft: 1650,
    priceCr: 1.98,
    whyHere: ["Under comfortable EMI", "More storage"],
    hero: "Quieter pocket, stronger monthly buffer — kitchen felt tighter on visit.",
    icon: "🪴",
  },
  {
    id: "park-retreat",
    name: "Godrej Park Retreat",
    short: "Park Retreat",
    area: "Sarjapur",
    bhk: "3 BHK",
    sqft: 1720,
    priceCr: 2.15,
    whyHere: ["Newer inventory", "Park views"],
    hero: "Park edge is real — commute and school stretch are the open questions.",
    icon: "🌳",
  },
];

/** Catalog of UI-selectable items that land in notebook notes when clicked. */
export const CATALOG: CatalogFact[] = [
  {
    id: "wf-schools",
    propertyId: "waterford",
    kind: "theme",
    mark: "fact",
    tag: "schools",
    label: "Nearby schools · strong cluster",
    detail: "3 CBSE schools within 1.8 km · matched your search",
    source: "Map · schools layer",
  },
  {
    id: "wf-water",
    propertyId: "waterford",
    kind: "fact",
    mark: "concern",
    tag: "water",
    label: "Summer tanker dependence reported",
    detail: "Resident reviews · 3 sources",
    source: "Reviews",
  },
  {
    id: "wf-oc",
    propertyId: "waterford",
    kind: "fact",
    mark: "fact",
    tag: "legal",
    label: "OC available for this tower",
    detail: "RERA-linked project file",
    source: "RERA",
  },
  {
    id: "wf-price",
    propertyId: "waterford",
    kind: "fact",
    mark: "concern",
    tag: "price",
    label: "Asking 6–9% above nearby evidence",
    detail: "Peer set · Whitefield 3BHK delivered",
    source: "Price proof",
  },
  {
    id: "wf-ht",
    propertyId: "waterford",
    kind: "fact",
    mark: "concern",
    tag: "commute",
    label: "High-tension corridor ~420 m",
    detail: "Transmission layer",
    source: "Map",
  },
  {
    id: "wf-layout",
    propertyId: "waterford",
    kind: "theme",
    mark: "fact",
    tag: "layout",
    label: "Type C · separate utility balcony",
    detail: "Floor plan shelf",
    source: "Plans",
  },
  {
    id: "da-schools",
    propertyId: "dream-acres",
    kind: "theme",
    mark: "fact",
    tag: "schools",
    label: "Nearby schools · moderate",
    detail: "2 schools within 2.4 km",
    source: "Map · schools layer",
  },
  {
    id: "da-water",
    propertyId: "dream-acres",
    kind: "fact",
    mark: "fact",
    tag: "water",
    label: "Cauvery + borewell mentioned",
    detail: "Builder + resident notes",
    source: "Reviews",
  },
  {
    id: "da-storage",
    propertyId: "dream-acres",
    kind: "theme",
    mark: "fact",
    tag: "layout",
    label: "More kitchen storage on visit",
    detail: "Compared with Waterford Type C",
    source: "Visit theme",
  },
  {
    id: "da-price",
    propertyId: "dream-acres",
    kind: "fact",
    mark: "fact",
    tag: "emi",
    label: "Inside comfortable EMI band",
    detail: "Uses your ₹1.35 L comfort line",
    source: "Plan",
  },
  {
    id: "pr-park",
    propertyId: "park-retreat",
    kind: "theme",
    mark: "fact",
    tag: "open-space",
    label: "Park-facing inventory",
    detail: "Society open-space shelf",
    source: "Society",
  },
  {
    id: "pr-commute",
    propertyId: "park-retreat",
    kind: "fact",
    mark: "concern",
    tag: "commute",
    label: "Sarjapur stretch · peak uncertainty",
    detail: "Traffic layer · weekday evenings",
    source: "Map",
  },
  {
    id: "pr-schools",
    propertyId: "park-retreat",
    kind: "theme",
    mark: "question",
    tag: "schools",
    label: "School access · still open",
    detail: "No strong cluster inside 2 km yet",
    source: "Map · schools layer",
  },
  {
    id: "wf-complaint",
    propertyId: "waterford",
    kind: "fact",
    mark: "concern",
    tag: "legal",
    label: "2 RERA complaints on record",
    detail: "Buyer complaints · portal extract",
    source: "RERA",
  },
  {
    id: "wf-completion",
    propertyId: "waterford",
    kind: "fact",
    mark: "fact",
    tag: "legal",
    label: "Proposed completion · Dec 2019 · delivered lag noted",
    detail: "RERA project timeline",
    source: "RERA",
  },
];

/** Long RERA complaint body for select-text → Remember demo. */
export const RERA_COMPLAINT_BODY = `Complaint 1 (2023-11): Allottee states common-area waterproofing remains incomplete on Tower C terrace after possession. Builder reply claims snag list closed; allottee disputes and asks RERA to direct completion with receipts.

Complaint 2 (2024-02): Delay in society formation and handover of corpus documents. Allottee requests clarity on maintenance escrow and OC coverage for this tower. Portal shows status Open.`;

export const PLAN_PINS: CatalogFact[] = [
  {
    id: "money-down",
    propertyId: "waterford",
    kind: "plan",
    mark: "money",
    tag: "down-payment",
    label: "Available down payment · ₹58 L",
    detail: "Buyer-level · applies to all homes",
    source: "Plan assumptions",
  },
  {
    id: "money-emi",
    propertyId: "waterford",
    kind: "plan",
    mark: "money",
    tag: "emi",
    label: "Comfortable EMI · ₹1.35 L / month",
    detail: "Buyer-level comfort line",
    source: "Plan assumptions",
  },
  {
    id: "wf-gap",
    propertyId: "waterford",
    kind: "plan",
    mark: "money",
    tag: "down-payment",
    label: "Needs ~₹62 L · gap ₹4 L",
    detail: "Required vs your ₹58 L",
    source: "Plan · Waterford",
  },
  {
    id: "da-buffer",
    propertyId: "dream-acres",
    kind: "plan",
    mark: "money",
    tag: "down-payment",
    label: "Needs ~₹44 L · buffer ₹14 L",
    detail: "Required vs your ₹58 L",
    source: "Plan · Dream Acres",
  },
];

export function propertyById(id: PropertyId): PropertySeed {
  return PROPERTIES.find((p) => p.id === id)!;
}

export function tagById(id: TagId): TagDef {
  return TAGS.find((t) => t.id === id)!;
}

export function formatCr(n: number): string {
  return `₹${n.toFixed(2)} Cr`;
}

export function markGlyph(mark: NoteMark): string {
  switch (mark) {
    case "fact":
      return "✓";
    case "concern":
      return "!";
    case "question":
      return "?";
    case "money":
      return "₹";
    default:
      return "○";
  }
}

/** Soft guess for handwritten notes — user can still change tag. */
export function guessTag(text: string): TagId {
  const t = text.toLowerCase();
  if (/down\s*payment|downpayment|₹?\s*\d+\s*l|pf|gap/.test(t)) return "down-payment";
  if (/emi|monthly|outflow/.test(t)) return "emi";
  if (/school|cbse|bus/.test(t)) return "schools";
  if (/water|tanker|kaveri|cauvery|bore/.test(t)) return "water";
  if (/kitchen|layout|balcony|utility|storage|bedroom/.test(t)) return "layout";
  if (/oc|rera|legal|document|advocate/.test(t)) return "legal";
  if (/price|asking|above|below|comparable/.test(t)) return "price";
  if (/commute|traffic|sarjapur|office/.test(t)) return "commute";
  if (/park|open space|green/.test(t)) return "open-space";
  return "visit";
}
