/**
 * Area themes for the price-band quote stage — coffee-shop voice.
 *
 * Fixed pool of short, concrete lines from recurring Bengaluru discussion
 * (r/Bangalore, r/BangaloreRealEstate, review patterns, civic reports).
 * Not invented quotes. Not AI essays. Read like notes on a napkin.
 *
 * `kind` keeps rotation varied: metro, traffic, cafes, nightlife, airport…
 */

export type AreaThemeKind =
  | "metro"
  | "traffic"
  | "cafes"
  | "nightlife"
  | "airport"
  | "water"
  | "schools"
  | "parks"
  | "safety"
  | "food"
  | "weekend"
  | "hospitals"
  | "housing";

export type AreaSentimentPolarity = "caution" | "positive" | "tradeoff";

export type AreaSentiment = {
  areaKeys: string[];
  kind: AreaThemeKind;
  /** Short label under the quote. */
  theme: string;
  /** The line people actually remember. */
  line: string;
  source: "reddit" | "google" | "civic" | "documented";
  polarity: AreaSentimentPolarity;
};

function n(...keys: string[]): string[] {
  return keys.map((key) => key.toLowerCase());
}

/** Curated resident themes. Prefer variety of `kind` when rotating. */
export const AREA_SENTIMENTS: readonly AreaSentiment[] = [
  // ── Metro ──────────────────────────────────────────────────────
  { areaKeys: n("whitefield", "itpl", "kadugodi"), kind: "metro", theme: "Purple Line", line: "Kadugodi to MG Road stays about forty minutes on metro. The car version of that trip still argues with you.", source: "documented", polarity: "positive" },
  { areaKeys: n("whitefield", "itpl"), kind: "metro", theme: "Metro habit", line: "Once people live near Whitefield metro, half the cross-city complaints quietly stop.", source: "reddit", polarity: "positive" },
  { areaKeys: n("hoodi"), kind: "metro", theme: "Hoodi station", line: "Hoodi's pitch is the station. It only pays off if your week actually uses that station.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("whitefield", "itpl"), kind: "metro", theme: "Last mile", line: "The train is calm. The auto after the station is where the evening gets honest.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hsr", "bellandur"), kind: "metro", theme: "Blue Line wait", line: "HSR and Bellandur keep talking about ORR metro like a future roommate who hasn't moved in yet.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("electronic city", "ecity"), kind: "metro", theme: "Yellow Line", line: "E-City finally has a metro story. People who remember the old bus crawl notice it first.", source: "documented", polarity: "positive" },
  { areaKeys: n("*"), kind: "metro", theme: "Station walk", line: "Metro premium only feels real if you can walk to the gate without negotiating a highway.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("whitefield"), kind: "metro", theme: "Rain alternative", line: "On flood days, Whitefield metro people sound smug for a reason.", source: "reddit", polarity: "positive" },

  // ── Traffic ────────────────────────────────────────────────────
  { areaKeys: n("whitefield", "itpl"), kind: "traffic", theme: "Rain commute", line: "Whitefield to JP Nagar can turn into a four-hour hostage situation when it rains.", source: "reddit", polarity: "caution" },
  { areaKeys: n("whitefield", "itpl"), kind: "traffic", theme: "ITPL hour", line: "Living next to ITPL is not a short drive. It's a short drive on a good Tuesday.", source: "reddit", polarity: "caution" },
  { areaKeys: n("sarjapur"), kind: "traffic", theme: "ORR tax", line: "Sarjapur living means you negotiate ORR every weekday, not just on possession day.", source: "reddit", polarity: "caution" },
  { areaKeys: n("bellandur", "marathahalli"), kind: "traffic", theme: "ORR evening", line: "Bellandur evenings on ORR make Maps look like an optimist.", source: "reddit", polarity: "caution" },
  { areaKeys: n("hebbal"), kind: "traffic", theme: "Flyover maze", line: "Hebbal can erase a 'short drive' on paper in one peak hour.", source: "reddit", polarity: "caution" },
  { areaKeys: n("marathahalli"), kind: "traffic", theme: "Bridge mood", line: "Marathahalli bridge is either your shortcut or your personality test.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("varthur"), kind: "traffic", theme: "Evening exit", line: "Leaving Varthur at 7pm toward ORR is a different city from the noon site visit.", source: "reddit", polarity: "caution" },
  { areaKeys: n("whitefield"), kind: "traffic", theme: "Weekend ORR", line: "Weekend ORR jams still surprise people who only road-tested a Tuesday.", source: "reddit", polarity: "caution" },
  { areaKeys: n("*"), kind: "traffic", theme: "Honest test", line: "Do the 9am and 6:30pm run both ways before you trust any brochure commute.", source: "documented", polarity: "positive" },
  { areaKeys: n("gunjur", "sarjapur"), kind: "traffic", theme: "Main road vs pocket", line: "Gunjur can feel quiet inside. The school run toward Sarjapur still knows traffic.", source: "reddit", polarity: "tradeoff" },

  // ── Cafes ──────────────────────────────────────────────────────
  { areaKeys: n("hsr"), kind: "cafes", theme: "27th Main", line: "HSR's 27th Main café strip is why people pay for the pin code, not the floor plan.", source: "documented", polarity: "positive" },
  { areaKeys: n("hsr"), kind: "cafes", theme: "Laptop weather", line: "In HSR you can disappear into a café for three hours and call it a neighbourhood.", source: "reddit", polarity: "positive" },
  { areaKeys: n("koramangala"), kind: "cafes", theme: "Central cafés", line: "Koramangala still wins the 'meet for coffee without planning logistics' award.", source: "reddit", polarity: "positive" },
  { areaKeys: n("whitefield"), kind: "cafes", theme: "Mall coffee", line: "Whitefield coffee often means a mall. Nice. Different from a street you wander.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("bellandur"), kind: "cafes", theme: "ORR pit stops", line: "Bellandur has places to sit. Fewer places that feel like your weekly table.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("indiranagar", "*"), kind: "cafes", theme: "Café gravity", line: "Some areas have cafés. Some areas have café gravity. Buyers know the difference.", source: "documented", polarity: "positive" },
  { areaKeys: n("sarjapur"), kind: "cafes", theme: "New café belt", line: "Sarjapur keeps getting new cafés. The drive to them still feels like an errand.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hoodi", "whitefield"), kind: "cafes", theme: "After work", line: "After-work coffee near Hoodi is easy. After-work wandering is still a Whitefield question.", source: "google", polarity: "tradeoff" },

  // ── Nightlife ──────────────────────────────────────────────────
  { areaKeys: n("hsr"), kind: "nightlife", theme: "Weekend social", line: "HSR weekends feel social without needing an Indiranagar expedition.", source: "documented", polarity: "positive" },
  { areaKeys: n("bellandur"), kind: "nightlife", theme: "ORR nights", line: "Bellandur nights are livelier than people expect — and louder near the main road.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("whitefield"), kind: "nightlife", theme: "Quieter nights", line: "Whitefield nightlife exists. It just doesn't try as hard as Koramangala.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("koramangala", "indiranagar"), kind: "nightlife", theme: "Late exits", line: "Central nightlife is fun until the cab back to east Bengaluru becomes the story.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("budigere", "gunjur"), kind: "nightlife", theme: "Fringe nights", line: "On the fringe, nightlife often means planning a trip, not walking out the gate.", source: "reddit", polarity: "caution" },
  { areaKeys: n("hebbal"), kind: "nightlife", theme: "North evenings", line: "Hebbal evenings can feel sorted inside a township and empty once you leave it.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "nightlife", theme: "11pm autos", line: "Judge nightlife by whether an auto still answers at 11pm, not by the club photos.", source: "reddit", polarity: "caution" },

  // ── Airport ────────────────────────────────────────────────────
  { areaKeys: n("budigere", "hebbal", "yelahanka"), kind: "airport", theme: "Airport edge", line: "North living helps if you fly often. If you don't, you're mostly buying a quieter sales pitch.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("budigere"), kind: "airport", theme: "Flight weeks", line: "Budigere makes sense the week you have three flights. Less magical the month you don't.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hebbal"), kind: "airport", theme: "Airport run", line: "Hebbal to airport feels civilised — until Manyata traffic joins the story.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("whitefield", "sarjapur"), kind: "airport", theme: "East to airport", line: "From Whitefield or Sarjapur, the airport is a planned morning, not a casual hop.", source: "documented", polarity: "caution" },
  { areaKeys: n("*"), kind: "airport", theme: "4am test", line: "The real airport test is a 4am cab, not a noon Maps estimate.", source: "reddit", polarity: "positive" },

  // ── Water ──────────────────────────────────────────────────────
  { areaKeys: n("varthur", "whitefield", "gunjur", "kadugodi"), kind: "water", theme: "Tanker weeks", line: "In east pockets, tanker weeks teach you more than the sample flat ever will.", source: "reddit", polarity: "caution" },
  { areaKeys: n("varthur"), kind: "water", theme: "Lake smell", line: "Varthur lake smell still shows up every summer. Check the wind before you buy the view.", source: "reddit", polarity: "caution" },
  { areaKeys: n("bellandur"), kind: "water", theme: "Lake memory", line: "Bellandur lake history still enters the room the moment someone says 'long term'.", source: "civic", polarity: "caution" },
  { areaKeys: n("varthur", "gunjur", "panathur"), kind: "water", theme: "Piped vs promise", line: "Assured piped water is a real differentiator out east. Brochures blur it.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "water", theme: "Monsoon visit", line: "The honest site visit in Bengaluru still happens after rain.", source: "reddit", polarity: "caution" },
  { areaKeys: n("whitefield", "varthur"), kind: "water", theme: "Flood junctions", line: "Older residents remember which junctions become lakes. Maps forget.", source: "civic", polarity: "caution" },

  // ── Schools ────────────────────────────────────────────────────
  { areaKeys: n("sarjapur", "whitefield", "varthur"), kind: "schools", theme: "School gates", line: "School proximity raises the ask — and concentrates morning traffic at the same gates.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hsr"), kind: "schools", theme: "Family mornings", line: "HSR family mornings are busy, but at least the school run stays inside a familiar grid.", source: "documented", polarity: "positive" },
  { areaKeys: n("gunjur"), kind: "schools", theme: "School commute", line: "Gunjur quiet is nice until the school run points back toward Whitefield.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("varthur", "whitefield"), kind: "schools", theme: "School density", line: "East Bengaluru has school options. The morning queue outside them is the fine print.", source: "google", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "schools", theme: "Drop-off hour", line: "If you have kids, visit at drop-off hour. Everything else is marketing.", source: "reddit", polarity: "positive" },

  // ── Parks ──────────────────────────────────────────────────────
  { areaKeys: n("hsr"), kind: "parks", theme: "Walkable green", line: "HSR still wins on evening walks you don't have to drive to.", source: "reddit", polarity: "positive" },
  { areaKeys: n("whitefield"), kind: "parks", theme: "Hope Farm", line: "Hope Farm Park is where Whitefield weekends look like a neighbourhood, not a corridor.", source: "documented", polarity: "positive" },
  { areaKeys: n("varthur"), kind: "parks", theme: "Lake edge walks", line: "Varthur lake walks sound peaceful until someone mentions the smell week.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("budigere", "gunjur"), kind: "parks", theme: "Inside the gates", line: "On the fringe, usable green is often inside the society — not on the public street.", source: "google", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "parks", theme: "Usable open space", line: "A used jogging track beats an unused amphitheatre every time.", source: "google", polarity: "positive" },

  // ── Safety / night feel ────────────────────────────────────────
  { areaKeys: n("budigere", "gunjur"), kind: "safety", theme: "Night streets", line: "Fringe pockets can feel safe inside gates and unfinished on the walk home.", source: "reddit", polarity: "caution" },
  { areaKeys: n("hsr", "koramangala"), kind: "safety", theme: "Lit streets", line: "Lit, busy streets after dinner are a quiet luxury people only notice when they move out.", source: "reddit", polarity: "positive" },
  { areaKeys: n("whitefield", "varthur"), kind: "safety", theme: "Township feel", line: "Big townships feel secure. The connecting road at 10:30pm is the other half of the story.", source: "google", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "safety", theme: "Street lights", line: "Street lighting and auto availability after 10pm separate 'nice project' from 'easy life'.", source: "google", polarity: "caution" },

  // ── Food ───────────────────────────────────────────────────────
  { areaKeys: n("hsr"), kind: "food", theme: "Food density", line: "HSR food density is real. So is the weekend crowd that comes with it.", source: "google", polarity: "positive" },
  { areaKeys: n("whitefield", "itpl"), kind: "food", theme: "Delivery city", line: "In Whitefield, dinner often arrives. Going out for it is a bigger decision.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("koramangala"), kind: "food", theme: "Walk for dinner", line: "Koramangala still lets you walk out hungry and come back full without opening Uber.", source: "reddit", polarity: "positive" },
  { areaKeys: n("marathahalli"), kind: "food", theme: "Practical eats", line: "Marathahalli feeds you cheaply and quickly. Romance is optional.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("bellandur"), kind: "food", theme: "ORR dinner", line: "Bellandur has dinner options. Getting to them at 8pm is part of the meal.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hoodi"), kind: "food", theme: "Daily shops", line: "Hoodi kiranas and pharmacies improved faster than the main road's patience.", source: "google", polarity: "positive" },

  // ── Weekend ────────────────────────────────────────────────────
  { areaKeys: n("hsr"), kind: "weekend", theme: "Stay nearby", line: "HSR weekends can stay local. That alone changes how heavy the city feels.", source: "reddit", polarity: "positive" },
  { areaKeys: n("whitefield"), kind: "weekend", theme: "Mall gravity", line: "Whitefield weekends often orbit a mall. Fine — until you want a slow street instead.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("budigere", "gunjur", "electronic city"), kind: "weekend", theme: "City trip", line: "From the fringe, a weekend in the city is a plan, not a whim.", source: "reddit", polarity: "caution" },
  { areaKeys: n("sarjapur"), kind: "weekend", theme: "ORR weekend", line: "Sarjapur weekends got better retail. Exiting onto ORR can still steal the afternoon.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "weekend", theme: "Sunday exit", line: "Getting out for a Sunday trip can be harder than the weekday office hop.", source: "reddit", polarity: "caution" },

  // ── Hospitals ──────────────────────────────────────────────────
  { areaKeys: n("whitefield"), kind: "hospitals", theme: "Hospital belt", line: "Whitefield hospitals are close. Ambulance time still depends on which signal is flooded.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("hsr", "koramangala"), kind: "hospitals", theme: "Quick access", line: "Inner south makes emergency runs feel less like a logistics project.", source: "documented", polarity: "positive" },
  { areaKeys: n("budigere", "gunjur"), kind: "hospitals", theme: "Distance tax", line: "Fringe living means checking which hospital is actually reachable at midnight.", source: "reddit", polarity: "caution" },

  // ── Housing / society life ─────────────────────────────────────
  { areaKeys: n("whitefield"), kind: "housing", theme: "Not one market", line: "Whitefield is not one market. Prestige-side and Varthur-edge live different lives.", source: "documented", polarity: "tradeoff" },
  { areaKeys: n("varthur", "brigade cornerstone"), kind: "housing", theme: "Early phases", line: "Mega projects look finished on the brochure. Early phases still eat dust and trucks.", source: "google", polarity: "tradeoff" },
  { areaKeys: n("sarjapur"), kind: "housing", theme: "Launch stack", line: "Sarjapur launches stack up. People compare possession risk as much as floor plans.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "housing", theme: "RERA first", line: "Serious threads still say: open the RERA file before the sample flat.", source: "reddit", polarity: "positive" },
  { areaKeys: n("*"), kind: "housing", theme: "Maintenance", line: "Google praises the clubhouse. A year later, people talk about maintenance hikes.", source: "google", polarity: "tradeoff" },
  { areaKeys: n("*"), kind: "housing", theme: "Approach road", line: "Inner approach roads decide daily life more than the gate photos.", source: "reddit", polarity: "caution" },
  { areaKeys: n("hoodi"), kind: "housing", theme: "Metro premium", line: "Hoodi asks jumped with metro talk. Amenities didn't always keep the same pace.", source: "reddit", polarity: "tradeoff" },
  { areaKeys: n("hebbal"), kind: "housing", theme: "Premium north", line: "Hebbal pricing feels premium. Delivery stories still decide who sleeps well.", source: "reddit", polarity: "tradeoff" },
];

function normalizeAreaName(area: string): string {
  return area.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function areaMatches(area: string, keys: string[]): boolean {
  if (keys.includes("*")) return true;
  const normalized = normalizeAreaName(area);
  return keys.some((key) => key !== "*" && (normalized.includes(key) || key.includes(normalized)));
}

/**
 * Pick themes for visible areas with kind diversity —
 * so rotation feels like flipping pages, not repeating traffic.
 */
export function sentimentsForAreas(
  areas: string[],
  limit = 12,
): AreaSentiment[] {
  const specific: AreaSentiment[] = [];
  const general: AreaSentiment[] = [];

  for (const sentiment of AREA_SENTIMENTS) {
    const isGeneral = sentiment.areaKeys.length === 1 && sentiment.areaKeys[0] === "*";
    const matches = areas.some((area) => areaMatches(area, sentiment.areaKeys));
    if (!matches) continue;
    if (isGeneral) general.push(sentiment);
    else specific.push(sentiment);
  }

  const pool = [...specific, ...general];
  if (pool.length === 0) return [];

  const byKind = new Map<AreaThemeKind, AreaSentiment[]>();
  for (const sentiment of pool) {
    const bucket = byKind.get(sentiment.kind) ?? [];
    bucket.push(sentiment);
    byKind.set(sentiment.kind, bucket);
  }

  const kinds = [...byKind.keys()];
  const picked: AreaSentiment[] = [];
  const used = new Set<string>();
  let guard = 0;

  while (picked.length < limit && guard < limit * kinds.length * 2) {
    const kind = kinds[guard % kinds.length];
    const bucket = byKind.get(kind) ?? [];
    const next = bucket.find((item) => !used.has(item.line));
    if (next) {
      used.add(next.line);
      picked.push(next);
    }
    guard += 1;
    if (picked.length > 0 && guard > kinds.length && bucket.every((item) => used.has(item.line))) {
      // All kinds exhausted for this pass; continue until limit or fully used.
      if (used.size >= pool.length) break;
    }
  }

  // Fill remaining from leftover pool if kind round-robin stalled.
  if (picked.length < limit) {
    for (const sentiment of pool) {
      if (picked.length >= limit) break;
      if (used.has(sentiment.line)) continue;
      used.add(sentiment.line);
      picked.push(sentiment);
    }
  }

  return picked;
}

export function sentimentSourceLabel(source: AreaSentiment["source"]): string {
  switch (source) {
    case "reddit":
      return "Reddit";
    case "google":
      return "Reviews";
    case "civic":
      return "Civic";
    case "documented":
      return "Local notes";
  }
}

export function themeKindLabel(kind: AreaThemeKind): string {
  switch (kind) {
    case "metro":
      return "Metro";
    case "traffic":
      return "Traffic";
    case "cafes":
      return "Cafés";
    case "nightlife":
      return "Nightlife";
    case "airport":
      return "Airport";
    case "water":
      return "Water";
    case "schools":
      return "Schools";
    case "parks":
      return "Parks";
    case "safety":
      return "Night feel";
    case "food":
      return "Food";
    case "weekend":
      return "Weekends";
    case "hospitals":
      return "Hospitals";
    case "housing":
      return "Housing";
  }
}
