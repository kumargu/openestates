import { initialPropertySceneUrls } from "./propertyScene.ts";
import { hasAroundThisHomePlate } from "./nearbyPlateProjection.ts";
import type {
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

export type StoryArrivalModel = {
  frames: StoryMediaFrame[];
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
  href: string;
  availability: "complete" | "partial";
  registrationIds: string[];
};

export type StoryComparison = {
  id: string;
  title: string;
  area: string;
  price?: number;
  status?: string;
};

export type StoryDecisionModel = {
  canSave: boolean;
  canNote: boolean;
  galleryCount: number;
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
  | (StoryDeckBase & { kind: "compare" })
  | (StoryDeckBase & { kind: "decision" });

export type PropertyStoryModel = {
  identity: StoryIdentity;
  media: StoryMedia;
  map: StoryMapModel;
  arrival: StoryArrivalModel;
  reviews: StoryReviewsModel;
  recordCards: StoryRecordCard[];
  comparisons: StoryComparison[];
  decision: StoryDecisionModel;
  coverage: StoryCoverage;
  motionSeed: number;
  motionTheme: StoryMotionTheme;
  decks: PropertyStoryDeck[];
};

export type PropertyStoryProjectionOptions = {
  media?: StoryMediaFrameInput[];
  motionTheme?: StoryMotionTheme;
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
  "decision",
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
  });
  const suppliedByUrl = new Map(inputs?.map((frame) => [frame.url, frame]));
  const urls = inputs
    ? initialPropertySceneUrls({
        heroImage: inputs[0]?.url,
        images: inputs.slice(1).map((frame) => frame.url),
      })
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
  const frames =
    data.evidence?.sections
      .flatMap((section) => section.media ?? [])
      .flatMap((strip) =>
        strip.frames.map((frame, index) => ({
          id: `arrival-${stableStoryHash(`${strip.kind}:${frame.image_url}:${index}`)
            .toString(16)}`,
          url: frame.image_url,
          role: "neighbourhood" as const,
          sourceType: strip.provider,
          lifecycle: "unknown" as const,
          capturedAt: frame.capture_date || undefined,
          sourceUrl: frame.source_url || undefined,
        })),
      ) ?? [];
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
  return [{
    id: "rera",
    label: "RERA report",
    href: report.href,
    availability:
      report.availability === "available" ? "complete" : "partial",
    registrationIds: [...report.registration_ids].sort(),
  }];
}

function projectComparisons(
  properties: PropertyCard[],
): StoryComparison[] {
  return properties.slice(0, 3).map((property) => ({
    id: property.id,
    title: property.title,
    area: property.area,
    price: hasKnownNumber(property.price) ? property.price : undefined,
    status:
      property.home_state_display ||
      property.project_status_display ||
      property.possession_status ||
      undefined,
  }));
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
  if (proposedCount > input.frames.length / 2 || architecturalCount >= 2) {
    return "architectural-drift";
  }
  if (input.frames.length >= 5) return "editorial-cut";

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
    {
      id: "decision",
      kind: "decision",
      primaryFactKeys: [],
    },
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
  const comparisons = projectComparisons(data.similar_properties);
  const map = {
    available: hasAroundThisHomePlate(data.map_context ?? null),
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
    decision: {
      canSave: true,
      canNote: true,
      galleryCount: media.galleryUrls.length,
    },
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

export function shouldAutoAdvanceStory(input: {
  playing: boolean;
  frameCount: number;
  reducedMotion: boolean;
  isVisible: boolean;
  documentVisible: boolean;
}): boolean {
  return (
    input.playing &&
    input.frameCount > 1 &&
    !input.reducedMotion &&
    input.isVisible &&
    input.documentVisible
  );
}
