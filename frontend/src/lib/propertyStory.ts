import { initialPropertySceneUrls } from "./propertyScene.ts";
import { backendUrl } from "./runtimeConfig.ts";
import { hasAroundThisHomePlate } from "./nearbyPlateProjection.ts";
import { visibleEvidenceSections } from "./evidence.ts";
import { workspaceCompareHref } from "./workspaceNav.ts";
import type {
  DecisionLabel,
  PropertyCard,
  PropertyDetailResponse,
} from "./types.ts";

export type StoryMotionTheme =
  | "quiet-pan"
  | "architectural-drift"
  | "slow-push"
  | "editorial-cut"
  | "still"
  | "reduced";

export type StoryMediaLifecycle = "current" | "proposed" | "unknown";

export type StoryMediaRole =
  | "hero"
  | "exterior"
  | "building"
  | "amenity"
  | "neighbourhood"
  | "gallery"
  | "unknown";

export type StoryFocalPoint = {
  x: number;
  y: number;
};

export type StoryMediaFrame = {
  id: string;
  url: string;
  role: StoryMediaRole;
  sourceType: string;
  lifecycle: StoryMediaLifecycle;
  capturedAt?: string;
  sourceUrl?: string;
  focalPoint?: StoryFocalPoint;
};

export type StoryMediaFrameInput = {
  id?: string;
  url: string;
  role?: StoryMediaRole;
  sourceType?: string;
  lifecycle?: StoryMediaLifecycle;
  capturedAt?: string;
  sourceUrl?: string;
  focalPoint?: StoryFocalPoint;
};

export type StoryIdentityFact = {
  key: string;
  value: string;
};

export type StoryIdentity = {
  propertyId: string;
  title: string;
  location: string;
  facts: StoryIdentityFact[];
};

export type StoryMedia = {
  frames: StoryMediaFrame[];
  galleryUrls: string[];
};

export type StoryMapModel = {
  available: boolean;
};

export type StoryArrivalFrame = {
  id: string;
  url: string;
  label: string;
  distanceFromGateM?: number;
  heading?: number;
  sourceType: string;
  lifecycle: StoryMediaLifecycle;
  capturedAt?: string;
  sourceUrl?: string;
  stripKind: string;
};

export type StoryArrivalModel = {
  frames: StoryArrivalFrame[];
};

export type StoryReviewsModel = {
  state: "present" | "unresolved" | "missing";
  rating?: number;
  count?: number;
  url?: string;
};

export type StoryRecordCard = {
  id: string;
  label: string;
  title: string;
  href: string;
  availability: "available" | "partial";
  registrationIds: string[];
  facts: Array<{
    key: string;
    label: string;
    value?: string;
  }>;
};

export type StoryComparison = {
  id: string;
  title: string;
  area: string;
  bhk?: number;
  price?: number;
  sizeLabel?: string;
  status?: string;
  societyName?: string;
  heroImage?: string;
  googleRating?: number;
  isCurrent: boolean;
};

export type StoryCoverage = {
  level: "rich" | "partial" | "sparse";
  availableDecks: number;
  totalDecks: number;
};

type StoryDeckBase = {
  id: string;
  primaryFactKeys: string[];
};

export type PropertyStoryDeck =
  | (StoryDeckBase & { kind: "hero" })
  | (StoryDeckBase & { kind: "map" })
  | (StoryDeckBase & { kind: "arrival" })
  | (StoryDeckBase & { kind: "reviews" })
  | (StoryDeckBase & { kind: "record" })
  | (StoryDeckBase & { kind: "compare" });

export type PropertyStoryModel = {
  identity: StoryIdentity;
  media: StoryMedia;
  map: StoryMapModel;
  arrival: StoryArrivalModel;
  reviews: StoryReviewsModel;
  recordCards: StoryRecordCard[];
  comparisons: StoryComparison[];
  compareHref?: string;
  coverage: StoryCoverage;
  motionSeed: number;
  motionTheme: StoryMotionTheme;
  decks: PropertyStoryDeck[];
};

export type PropertyStoryProjectionOptions = {
  media?: StoryMediaFrameInput[];
  motionTheme?: StoryMotionTheme;
  mapAvailable?: boolean;
  comparisonProperties?: PropertyCard[];
  recommendationProperties?: PropertyCard[];
};

export type StoryMotionDefinition = {
  durationMs: number;
  transitionMs: number;
  className: string;
};

export const STORY_MOTION_REGISTRY: Record<
  StoryMotionTheme,
  StoryMotionDefinition
> = {
  "quiet-pan": {
    durationMs: 7_600,
    transitionMs: 1_200,
    className: "story-motion--quiet-pan",
  },
  "architectural-drift": {
    durationMs: 8_200,
    transitionMs: 1_400,
    className: "story-motion--architectural-drift",
  },
  "slow-push": {
    durationMs: 7_800,
    transitionMs: 1_250,
    className: "story-motion--slow-push",
  },
  "editorial-cut": {
    durationMs: 5_800,
    transitionMs: 850,
    className: "story-motion--editorial-cut",
  },
  still: {
    durationMs: 0,
    transitionMs: 0,
    className: "story-motion--still",
  },
  reduced: {
    durationMs: 0,
    transitionMs: 0,
    className: "story-motion--reduced",
  },
};

const STORY_DECK_ORDER: PropertyStoryDeck["kind"][] = [
  "hero",
  "map",
  "arrival",
  "reviews",
  "record",
  "compare",
];

function hasKnownNumber(
  value: number | null | undefined,
): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function formatPrice(value: number): string {
  if (value >= 10_000_000) {
    const crores = value / 10_000_000;
    return `₹${crores.toFixed(1)} Cr`;
  }
  if (value >= 100_000) return `₹${(value / 100_000).toFixed(1)} L`;
  return `₹${value.toLocaleString("en-IN")}`;
}

function compactStatus(data: PropertyDetailResponse): string | undefined {
  const value =
    data.home_state_display ||
    data.project_status_display ||
    data.property.possession_status;
  const firstPhrase = value?.split("·")[0]?.trim();
  if (!firstPhrase) return undefined;
  return firstPhrase
    .replace(/^home state:\s*/i, "")
    .replace(/_/g, " ")
    .replace(/^\w/, (character) => character.toUpperCase());
}

function storyTitle(data: PropertyDetailResponse): string {
  const societyName = data.society?.name.trim();
  if (societyName) return societyName;
  const listingTitle = data.property.title.trim();
  const compactTitle = listingTitle.replace(
    /^\d+(?:\.\d+)?\s*BHK\s+(?:in|at)\s+/i,
    "",
  ).trim();
  const localityNames = [data.property.area, data.property.city]
    .filter(Boolean)
    .map((value) => value.trim().toLocaleLowerCase("en-IN"));
  if (
    !compactTitle
    || localityNames.includes(compactTitle.toLocaleLowerCase("en-IN"))
  ) {
    return listingTitle;
  }
  return compactTitle;
}

function identityFacts(data: PropertyDetailResponse): StoryIdentityFact[] {
  const property = data.property;
  const facts: Array<StoryIdentityFact | null> = [
    hasKnownNumber(property.price)
      ? { key: "price", value: formatPrice(property.price) }
      : null,
    hasKnownNumber(property.bhk)
      ? { key: "configuration", value: `${property.bhk} BHK` }
      : null,
    hasKnownNumber(property.carpet_area_sqft)
      ? {
          key: "size",
          value: `${property.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet`,
        }
      : hasKnownNumber(property.super_builtup_sqft)
        ? {
            key: "size",
            value: `${property.super_builtup_sqft.toLocaleString("en-IN")} sqft super built-up`,
          }
        : null,
    compactStatus(data)
      ? { key: "status", value: compactStatus(data) ?? "" }
      : null,
  ];
  return facts.filter((fact): fact is StoryIdentityFact => fact !== null);
}

export function stableStoryHash(value: string): number {
  let hash = 2_166_136_261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return hash >>> 0;
}

function focalPoint(value?: StoryFocalPoint): StoryFocalPoint | undefined {
  if (!value) return undefined;
  return {
    x: Math.max(0, Math.min(1, value.x)),
    y: Math.max(0, Math.min(1, value.y)),
  };
}

function sourceUrl(value: string): string | undefined {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:"
      ? url.toString()
      : undefined;
  } catch {
    return undefined;
  }
}

function projectMedia(
  data: PropertyDetailResponse,
  inputs?: StoryMediaFrameInput[],
): StoryMedia {
  const galleryUrls = initialPropertySceneUrls({
    heroImage: data.property.hero_image,
    images: data.property.images,
  }).map(backendUrl);
  const suppliedByUrl = new Map(inputs?.map((frame) => [frame.url, frame]));
  const urls = inputs
    ? initialPropertySceneUrls({
        heroImage: inputs[0]?.url,
        images: inputs.slice(1).map((frame) => frame.url),
      }).map(backendUrl)
    : galleryUrls;

  const frames = urls.slice(0, 7).map((url, index) => {
    const supplied = suppliedByUrl.get(url);
    return {
      id: supplied?.id ?? `story-media-${stableStoryHash(url).toString(16)}`,
      url,
      role: supplied?.role ?? (index === 0 ? "hero" : "gallery"),
      sourceType: supplied?.sourceType ?? "unknown",
      lifecycle: supplied?.lifecycle ?? "unknown",
      capturedAt: supplied?.capturedAt,
      sourceUrl: supplied?.sourceUrl ?? sourceUrl(data.property.source_reference),
      focalPoint: focalPoint(supplied?.focalPoint),
    } satisfies StoryMediaFrame;
  });

  return {
    frames,
    galleryUrls: inputs ? urls : galleryUrls,
  };
}

function projectArrival(data: PropertyDetailResponse): StoryArrivalModel {
  const approachSections = visibleEvidenceSections(
    data.evidence?.sections ?? [],
  ).filter((section) => section.kind === "approach_road");
  const frames = approachSections
    .flatMap((section) => section.media ?? [])
    .flatMap((strip) =>
      strip.frames
        .filter((frame) => Boolean(frame.image_url.trim()))
        .map((frame, index) => ({
          id: `arrival-${stableStoryHash(
            `${strip.kind}:${frame.image_url}:${frame.label}:${index}`,
          ).toString(16)}`,
          url: backendUrl(frame.image_url.trim()),
          label: frame.label.trim(),
          distanceFromGateM:
            Number.isFinite(frame.distance_from_gate_m)
            && frame.distance_from_gate_m >= 0
              ? frame.distance_from_gate_m
              : undefined,
          heading: Number.isFinite(frame.heading) ? frame.heading : undefined,
          sourceType: strip.provider.trim() || "unknown",
          lifecycle: strip.kind === "street_view_strip"
            ? "current" as const
            : "unknown" as const,
          capturedAt: frame.capture_date.trim() || undefined,
          sourceUrl: sourceUrl(frame.source_url),
          stripKind: strip.kind,
        })),
    )
    .slice(0, 6);
  return { frames };
}

function projectReviews(
  data: PropertyDetailResponse,
): StoryReviewsModel {
  const reviews = data.external_reviews;
  if (!reviews) return { state: "missing" };
  const hasReviewFacts =
    hasKnownNumber(reviews.google_rating) ||
    hasKnownNumber(reviews.google_review_count) ||
    Boolean(reviews.reviews?.length);
  return {
    state: hasReviewFacts ? "present" : reviews.google_reviews_url
      ? "unresolved"
      : "missing",
    rating: hasKnownNumber(reviews.google_rating)
      ? reviews.google_rating
      : undefined,
    count: hasKnownNumber(reviews.google_review_count)
      ? reviews.google_review_count
      : undefined,
    url: reviews.google_reviews_url,
  };
}

function projectRecordCards(
  data: PropertyDetailResponse,
): StoryRecordCard[] {
  const report = data.rera_report_ref;
  if (report.availability === "unavailable") return [];
  const summary = data.decision_check_summary;
  const registrationNumber =
    summary?.registrationNumberCompact?.trim()
    || summary?.registrationNumber?.trim()
    || report.registration_ids[0]?.trim();
  const cards: StoryRecordCard[] = registrationNumber
    ? [{
        id: "rera-registration",
        label: "Karnataka RERA",
        title: "Registration",
        href: report.href,
        availability: report.availability,
        registrationIds: [...report.registration_ids].sort(),
        facts: [{
          key: "registration",
          label: "Number",
          value: registrationNumber,
        }],
      }]
    : [];
  const documentLabels = [
    ...(summary?.groups?.find((group) => group.id === "documents")?.labels ?? []),
    ...(summary?.primaryLabels ?? []).filter(
      (label) => label.groupId === "documents",
    ),
  ];
  const documentFacts: StoryRecordCard["facts"] = [];
  const usedFactKeys = new Set<string>();
  for (const label of documentLabels) {
    if (label.groupId !== "documents" || !label.label.trim()) continue;
    if (usedFactKeys.has(label.key)) continue;
    const value = documentFactValue(label);
    if (!value) continue;
    usedFactKeys.add(label.key);
    documentFacts.push({
      key: label.key,
      label: label.label.trim(),
      value,
    });
    if (documentFacts.length >= 3) break;
  }
  if (documentFacts.length > 0) {
    cards.push({
      id: "rera-documents",
      label: "Official record",
      title: "Documents",
      href: report.href,
      availability: report.availability,
      registrationIds: [...report.registration_ids].sort(),
      facts: documentFacts,
    });
  }
  return cards;
}

function documentFactValue(label: DecisionLabel): string | undefined {
  const text = label.valueText?.trim();
  if (!text) return "Found";

  const normalized = text.toLocaleLowerCase("en-IN");
  const presenceFact = /(?:available|present|found)$/i.test(label.key)
    || /\b(?:available|present|found)\b/i.test(label.label);
  if (["0", "false", "no"].includes(normalized)) return undefined;
  if (["1", "true", "yes"].includes(normalized)) {
    return presenceFact ? "Available" : "Found";
  }
  if (/^\d+(?:\.0+)?$/.test(normalized) && presenceFact) {
    return "Available";
  }
  return text;
}

export function projectStoryComparison(
  property: PropertyCard,
  currentPropertyId?: string,
): StoryComparison {
  return {
    id: property.id,
    title: property.society_name.trim() || property.title,
    area: property.area,
    bhk: hasKnownNumber(property.bhk) ? property.bhk : undefined,
    price: hasKnownNumber(property.price) ? property.price : undefined,
    sizeLabel: hasKnownNumber(property.carpet_area_sqft)
      ? `${property.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet`
      : hasKnownNumber(property.super_builtup_sqft)
        ? `${property.super_builtup_sqft.toLocaleString("en-IN")} sqft super built-up`
        : hasKnownNumber(property.sqft)
          ? `${property.sqft.toLocaleString("en-IN")} sqft`
          : undefined,
    status:
      property.home_state_display
      || property.project_status_display
      || property.possession_status
      || undefined,
    societyName: property.society_name.trim() || undefined,
    heroImage: property.hero_image || property.images?.[0] || undefined,
    googleRating: hasKnownNumber(property.google_rating)
      ? property.google_rating
      : undefined,
    isCurrent: property.id === currentPropertyId,
  };
}

function projectComparisons(
  data: PropertyDetailResponse,
  comparisonProperties: PropertyCard[] = [],
  recommendationProperties: PropertyCard[] = [],
): { homes: StoryComparison[]; href?: string } {
  const currentSocietyName = data.society?.name.trim() || undefined;
  const current: StoryComparison = {
    id: data.property.id,
    title: currentSocietyName || storyTitle(data),
    area: data.property.area,
    bhk: hasKnownNumber(data.property.bhk) ? data.property.bhk : undefined,
    price: hasKnownNumber(data.property.price) ? data.property.price : undefined,
    sizeLabel: hasKnownNumber(data.property.carpet_area_sqft)
      ? `${data.property.carpet_area_sqft.toLocaleString("en-IN")} sqft carpet`
      : hasKnownNumber(data.property.super_builtup_sqft)
        ? `${data.property.super_builtup_sqft.toLocaleString("en-IN")} sqft super built-up`
        : undefined,
    status: compactStatus(data),
    societyName: currentSocietyName,
    heroImage: data.property.hero_image || data.property.images?.[0] || undefined,
    googleRating: hasKnownNumber(data.external_reviews?.google_rating)
      ? data.external_reviews?.google_rating
      : undefined,
    isCurrent: true,
  };
  const candidates = [
    ...comparisonProperties,
    ...recommendationProperties,
    ...(data.recommendation_branches ?? []).map((branch) => branch.property),
    ...data.similar_properties,
  ];
  const homes: StoryComparison[] = [current];
  const usedIds = new Set([current.id]);
  const usedSocieties = new Set([
    data.entity_refs?.society_entity_id
      || currentSocietyName?.toLocaleLowerCase()
      || current.title.toLocaleLowerCase(),
  ]);
  for (const property of candidates) {
    if (usedIds.has(property.id)) continue;
    const societyKey =
      property.kg_entity_refs?.society_entity_id
      || property.society_name.trim().toLocaleLowerCase()
      || property.title.trim().toLocaleLowerCase();
    if (usedSocieties.has(societyKey)) continue;
    usedIds.add(property.id);
    usedSocieties.add(societyKey);
    homes.push(projectStoryComparison(property, current.id));
    if (homes.length === 3) break;
  }
  if (homes.length !== 3) return { homes: [] };
  return {
    homes,
    href: workspaceCompareHref(
      homes.map((home) => home.id),
      current.id,
    ),
  };
}

export function selectStoryMotionTheme(input: {
  frames: StoryMediaFrame[];
  motionSeed: number;
  explicitTheme?: StoryMotionTheme;
  reducedMotion?: boolean;
}): StoryMotionTheme {
  if (input.reducedMotion) return "reduced";
  if (input.explicitTheme) return input.explicitTheme;
  if (input.frames.length <= 1) return "still";

  const proposedCount = input.frames.filter(
    (frame) => frame.lifecycle === "proposed",
  ).length;
  const architecturalCount = input.frames.filter((frame) =>
    ["building", "exterior"].includes(frame.role)
  ).length;
  if (proposedCount > input.frames.length / 2) {
    return "architectural-drift";
  }
  if (input.frames.length >= 5) {
    const galleryVariants: StoryMotionTheme[] = [
      "editorial-cut",
      "quiet-pan",
      "slow-push",
      "architectural-drift",
    ];
    return galleryVariants[input.motionSeed % galleryVariants.length]
      ?? "editorial-cut";
  }
  if (architecturalCount >= 2) {
    const architecturalVariants: StoryMotionTheme[] = [
      "architectural-drift",
      "quiet-pan",
      "slow-push",
      "editorial-cut",
    ];
    return architecturalVariants[
      input.motionSeed % architecturalVariants.length
    ] ?? "architectural-drift";
  }

  const variants: StoryMotionTheme[] = ["quiet-pan", "slow-push"];
  return variants[input.motionSeed % variants.length] ?? "quiet-pan";
}

function orderedDecks(input: {
  heroFactKeys: string[];
  hasMap: boolean;
  hasArrival: boolean;
  reviewState: StoryReviewsModel["state"];
  hasRecord: boolean;
  hasComparisons: boolean;
}): PropertyStoryDeck[] {
  const decks: PropertyStoryDeck[] = [
    {
      id: "property-cinema",
      kind: "hero",
      primaryFactKeys: input.heroFactKeys,
    },
    ...(input.hasMap
      ? [{
          id: "around-this-home",
          kind: "map" as const,
          primaryFactKeys: ["nearby_context"],
        }]
      : []),
    ...(input.hasArrival
      ? [{
          id: "remote-arrival",
          kind: "arrival" as const,
          primaryFactKeys: ["approach_road"],
        }]
      : []),
    ...(input.reviewState !== "missing"
      ? [{
          id: "resident-voice",
          kind: "reviews" as const,
          primaryFactKeys: ["external_reviews"],
        }]
      : []),
    ...(input.hasRecord
      ? [{
          id: "official-record",
          kind: "record" as const,
          primaryFactKeys: ["rera_registration"],
        }]
      : []),
    ...(input.hasComparisons
      ? [{
          id: "short-compare",
          kind: "compare" as const,
          primaryFactKeys: ["comparison_options"],
        }]
      : []),
  ];
  return decks.sort(
    (left, right) =>
      STORY_DECK_ORDER.indexOf(left.kind) -
      STORY_DECK_ORDER.indexOf(right.kind),
  );
}

export function projectPropertyStory(
  data: PropertyDetailResponse,
  options: PropertyStoryProjectionOptions = {},
): PropertyStoryModel {
  const facts = identityFacts(data);
  const media = projectMedia(data, options.media);
  const arrival = projectArrival(data);
  const reviews = projectReviews(data);
  const recordCards = projectRecordCards(data);
  const compare = projectComparisons(
    data,
    options.comparisonProperties,
    options.recommendationProperties,
  );
  const comparisons = compare.homes;
  const map = {
    available: options.mapAvailable
      ?? hasAroundThisHomePlate(data.map_context ?? null),
  };
  const motionSeed = stableStoryHash(data.property.id);
  const motionTheme = selectStoryMotionTheme({
    frames: media.frames,
    motionSeed,
    explicitTheme: options.motionTheme,
  });
  const decks = orderedDecks({
    heroFactKeys: facts.map((fact) => fact.key),
    hasMap: map.available,
    hasArrival: arrival.frames.length > 0,
    reviewState: reviews.state,
    hasRecord: recordCards.length > 0,
    hasComparisons: comparisons.length > 0,
  });
  const availableDecks = decks.length;
  const coverageLevel = availableDecks >= 6
    ? "rich"
    : availableDecks >= 4
      ? "partial"
      : "sparse";

  return {
    identity: {
      propertyId: data.property.id,
      title: storyTitle(data),
      location: [data.property.area, data.property.city]
        .filter(Boolean)
        .join(", "),
      facts,
    },
    media,
    map,
    arrival,
    reviews,
    recordCards,
    comparisons,
    compareHref: compare.href,
    coverage: {
      level: coverageLevel,
      availableDecks,
      totalDecks: STORY_DECK_ORDER.length,
    },
    motionSeed,
    motionTheme,
    decks,
  };
}

export function primaryStoryFactKeys(
  story: PropertyStoryModel,
): string[] {
  return story.decks.flatMap((deck) => deck.primaryFactKeys);
}

export function nextStoryFrameIndex(
  current: number,
  total: number,
): number {
  if (total <= 1) return 0;
  return (Math.max(0, Math.floor(current)) + 1) % total;
}

export function wrappedFilmstripOffset(
  index: number,
  activeIndex: number,
  total: number,
): number {
  if (total <= 1) return 0;
  let offset = index - activeIndex;
  const midpoint = total / 2;
  if (offset > midpoint) offset -= total;
  if (offset < -midpoint) offset += total;
  return Math.max(-3, Math.min(3, offset));
}

export function filmstripWindowIndices(
  frameCount: number,
  activeIndex: number,
): number[] {
  if (frameCount <= 0) return [];
  const active = ((Math.floor(activeIndex) % frameCount) + frameCount)
    % frameCount;
  if (frameCount === 1) return [active];
  const next = (active + 1) % frameCount;
  const previous = (active - 1 + frameCount) % frameCount;
  return [...new Set([active, next, previous])];
}

export function shouldAutoAdvanceStory(input: {
  playing: boolean;
  frameCount: number;
  reducedMotion: boolean;
  isVisible: boolean;
  documentVisible: boolean;
  durationMs?: number;
}): boolean {
  return (
    input.playing &&
    input.frameCount > 1 &&
    (input.durationMs === undefined || input.durationMs > 0) &&
    !input.reducedMotion &&
    input.isVisible &&
    input.documentVisible
  );
}
